use super::URefAddr;
use blake2::digest::{Input, VariableOutput};
use blake2::VarBlake2b;
use common::bytesrepr::{deserialize, ToBytes};
use common::key::{AccessRights, Key};
use common::value::account::Account;
use common::value::Value;
use execution::Error;
use rand::RngCore;
use rand_chacha::ChaChaRng;
use shared::newtypes::Validated;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;
use storage::global_state::{ExecutionEffect, StateReader};
use trackingcopy::{AddResult, TrackingCopy};

/// Holds information specific to the deployed contract.
pub struct RuntimeContext<'a, R: StateReader<Key, Value>> {
    state: Rc<RefCell<TrackingCopy<R>>>,
    // Enables look up of specific uref based on human-readable name
    uref_lookup: &'a mut BTreeMap<String, Key>,
    // Used to check uref is known before use (prevents forging urefs)
    known_urefs: HashMap<URefAddr, HashSet<AccessRights>>,
    account: &'a Account,
    args: Vec<Vec<u8>>,
    // Key pointing to the entity we are currently running
    //(could point at an account or contract in the global state)
    base_key: Key,
    gas_limit: u64,
    gas_counter: u64,
    fn_store_id: u32,
    rng: ChaChaRng,
}

impl<'a, R: StateReader<Key, Value>> RuntimeContext<'a, R>
where
    R::Error: Into<Error>,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        state: Rc<RefCell<TrackingCopy<R>>>,
        uref_lookup: &'a mut BTreeMap<String, Key>,
        known_urefs: HashMap<URefAddr, HashSet<AccessRights>>,
        args: Vec<Vec<u8>>,
        account: &'a Account,
        base_key: Key,
        gas_limit: u64,
        gas_counter: u64,
        fn_store_id: u32,
        rng: ChaChaRng,
    ) -> Self {
        RuntimeContext {
            state,
            uref_lookup,
            known_urefs,
            args,
            account,
            base_key,
            gas_limit,
            gas_counter,
            fn_store_id,
            rng,
        }
    }

    pub fn get_uref(&self, name: &str) -> Option<&Key> {
        self.uref_lookup.get(name)
    }

    pub fn fn_store_id(&self) -> u32 {
        self.fn_store_id
    }

    pub fn contains_uref(&self, name: &str) -> bool {
        self.uref_lookup.contains_key(name)
    }

    pub fn add_urefs(&mut self, urefs_map: HashMap<URefAddr, HashSet<AccessRights>>) {
        self.known_urefs.extend(urefs_map);
    }

    pub fn account(&self) -> &'a Account {
        self.account
    }

    pub fn args(&self) -> &Vec<Vec<u8>> {
        &self.args
    }

    pub fn rng(&self) -> &ChaChaRng {
        &self.rng
    }

    pub fn state(&self) -> Rc<RefCell<TrackingCopy<R>>> {
        Rc::clone(&self.state)
    }

    pub fn gas_limit(&self) -> u64 {
        self.gas_limit
    }

    pub fn gas_counter(&self) -> u64 {
        self.gas_counter
    }

    pub fn set_gas_counter(&mut self, new_gas_counter: u64) {
        self.gas_counter = new_gas_counter;
    }

    pub fn inc_fn_store_id(&mut self) {
        self.fn_store_id += 1;
    }

    pub fn base_key(&self) -> Key {
        self.base_key
    }

    /// Generates new function address.
    /// Function address is deterministic. It is a hash of public key, nonce and `fn_store_id`,
    /// which is a counter that is being incremented after every function generation.
    /// If function address was based only on account's public key and deploy's nonce,
    /// then all function addresses generated within one deploy would have been the same.
    pub fn new_function_address(&mut self) -> Result<[u8; 32], Error> {
        let mut pre_hash_bytes = Vec::with_capacity(44); //32 byte pk + 8 byte nonce + 4 byte ID
        pre_hash_bytes.extend_from_slice(self.account().pub_key());
        pre_hash_bytes.append(&mut self.account().nonce().to_bytes()?);
        pre_hash_bytes.append(&mut self.fn_store_id().to_bytes()?);

        self.inc_fn_store_id();

        let mut hasher = VarBlake2b::new(32).unwrap();
        hasher.input(&pre_hash_bytes);
        let mut hash_bytes = [0; 32];
        hasher.variable_result(|hash| hash_bytes.clone_from_slice(hash));
        Ok(hash_bytes)
    }

    pub fn new_uref(&mut self, value: Value) -> Result<Key, Error> {
        let mut key = [0u8; 32];
        self.rng.fill_bytes(&mut key);
        let key = Key::URef(key, AccessRights::READ_ADD_WRITE);
        let validated_key = Validated::new(key, Validated::valid)?;
        self.insert_uref(validated_key);
        self.write_gs(key, value)?;
        Ok(key)
    }

    /// Adds `key` to the map of named keys of current context.
    pub fn add_uref(&mut self, name: String, key: Key) -> Result<(), Error> {
        let base_key = self.base_key();
        self.add_gs(base_key, Value::NamedKey(name.clone(), key))?;
        let validated_key = Validated::new(key, Validated::valid)?;
        self.insert_named_uref(name, validated_key);
        Ok(())
    }

    pub fn read_gs(&mut self, key: &Key) -> Result<Option<Value>, Error> {
        let validated_key = Validated::new(*key, |key| {
            self.validate_readable(&key).and(self.validate_key(&key))
        })?;
        self.state
            .borrow_mut()
            .read(&validated_key)
            .map_err(Into::into)
    }

    pub fn write_gs(&mut self, key: Key, value: Value) -> Result<(), Error> {
        let validated_key: Validated<Key> = Validated::new(key, |key| {
            self.validated_writeable(&key).and(self.validate_key(&key))
        })?;
        let validated_value = Validated::new(value, |value| self.validate_keys(&value))?;
        self.state
            .borrow_mut()
            .write(validated_key, validated_value);
        Ok(())
    }

    pub fn store_contract(&mut self, contract: Value) -> Result<[u8; 32], Error> {
        let new_hash = self.new_function_address()?;
        let validated_value = Validated::new(contract, |cntr| self.validate_keys(&cntr))?;
        let validated_key = Validated::new(Key::Hash(new_hash), Validated::valid)?;
        self.state
            .borrow_mut()
            .write(validated_key, validated_value);
        Ok(new_hash)
    }

    pub fn insert_named_uref(&mut self, name: String, key: Validated<Key>) {
        self.insert_uref(key.clone());
        self.uref_lookup.insert(name, *key);
    }

    pub fn insert_uref(&mut self, key: Validated<Key>) {
        if let Key::URef(raw_addr, rights) = *key {
            let entry_rights = self
                .known_urefs
                .entry(raw_addr)
                .or_insert_with(|| std::iter::empty().collect());
            entry_rights.insert(rights);
        }
    }

    pub fn effect(&self) -> ExecutionEffect {
        self.state.borrow_mut().effect()
    }

    /// Validates whether keys used in the `value` are not forged.
    pub fn validate_keys(&self, value: &Value) -> Result<(), Error> {
        match value {
            Value::Int32(_)
            | Value::UInt128(_)
            | Value::UInt256(_)
            | Value::UInt512(_)
            | Value::ByteArray(_)
            | Value::ListInt32(_)
            | Value::String(_)
            | Value::ListString(_) => Ok(()),
            Value::NamedKey(_, key) => self.validate_key(&key),
            Value::Account(account) => {
                // This should never happen as accounts can't be created by contracts.
                // I am putting this here for the sake of completness.
                account
                    .urefs_lookup()
                    .values()
                    .try_for_each(|key| self.validate_key(key))
            }
            Value::Contract(contract) => contract
                .urefs_lookup()
                .values()
                .try_for_each(|key| self.validate_key(key)),
        }
    }

    /// Validates whether key is not forged (whether it can be found in the `known_urefs`)
    /// and whether the version of a key that contract wants to use, has access rights
    /// that are less powerful than access rights' of the key in the `known_urefs`.
    pub fn validate_key(&self, key: &Key) -> Result<(), Error> {
        match key {
            Key::URef(raw_addr, new_rights) => {
                self.known_urefs
                    .get(raw_addr) // Check if the `key` is known
                    .map(|known_rights| {
                        known_rights
                            .iter()
                            .any(|right| *right & *new_rights == *new_rights)
                    }) // are we allowed to use it this way?
                    .map(|_| ()) // at this point we know it's valid to use `key`
                    .ok_or_else(|| Error::ForgedReference(*key)) // otherwise `key` is forged
            }
            _ => Ok(()),
        }
    }

    pub fn deserialize_keys(&self, bytes: &[u8]) -> Result<Vec<Key>, Error> {
        let keys: Vec<Key> = deserialize(bytes)?;
        keys.iter().try_for_each(|k| self.validate_key(k))?;
        Ok(keys)
    }

    fn validate_readable(&self, key: &Key) -> Result<(), Error> {
        if self.is_readable(&key) {
            Ok(())
        } else {
            Err(Error::InvalidAccess {
                required: AccessRights::READ,
            })
        }
    }

    fn validated_addable(&self, key: &Key) -> Result<(), Error> {
        if self.is_addable(&key) {
            Ok(())
        } else {
            Err(Error::InvalidAccess {
                required: AccessRights::ADD,
            })
        }
    }

    fn validated_writeable(&self, key: &Key) -> Result<(), Error> {
        if self.is_writeable(&key) {
            Ok(())
        } else {
            Err(Error::InvalidAccess {
                required: AccessRights::WRITE,
            })
        }
    }

    // Tests whether reading from the `key` is valid.
    pub fn is_readable(&self, key: &Key) -> bool {
        match key {
            Key::Account(_) => &self.base_key() == key,
            Key::Hash(_) => true,
            Key::URef(_, rights) => rights.is_readable(),
        }
    }

    /// Tests whether addition to `key` is valid.
    pub fn is_addable(&self, key: &Key) -> bool {
        match key {
            Key::Account(_) | Key::Hash(_) => &self.base_key() == key,
            Key::URef(_, rights) => rights.is_addable(),
        }
    }

    // Test whether writing to `key` is valid.
    pub fn is_writeable(&self, key: &Key) -> bool {
        match key {
            Key::Account(_) | Key::Hash(_) => false,
            Key::URef(_, rights) => rights.is_writeable(),
        }
    }

    /// Adds `value` to the `key`. The premise for being able to `add` value is that
    /// the type of it [value] can be added (is a Monoid). If the values can't be added,
    /// either because they're not a Monoid or if the value stored under `key` has different type,
    /// then `TypeMismatch` errors is returned.
    pub fn add_gs(&mut self, key: Key, value: Value) -> Result<(), Error> {
        let validated_key = Validated::new(key, |k| {
            self.validated_addable(&k).and(self.validate_key(&k))
        })?;
        let validated_value = Validated::new(value, |v| self.validate_keys(&v))?;
        match self.state.borrow_mut().add(validated_key, validated_value) {
            Err(storage_error) => Err(storage_error.into()),
            Ok(AddResult::Success) => Ok(()),
            Ok(AddResult::KeyNotFound(key)) => Err(Error::KeyNotFound(key)),
            Ok(AddResult::TypeMismatch(type_mismatch)) => Err(Error::TypeMismatch(type_mismatch)),
            Ok(AddResult::Overflow) => Err(Error::Overflow),
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate common;
    extern crate failure;
    extern crate rand;
    extern crate rand_chacha;
    extern crate shared;
    extern crate storage;

    use super::{Error, RuntimeContext, TrackingCopy, URefAddr, Validated};
    use common::key::{AccessRights, Key};
    use common::value::{self, Account, Contract, Value};
    use execution::{create_rng, vec_key_rights_to_map};
    use rand::RngCore;
    use rand_chacha::ChaChaRng;
    use shared::newtypes::Blake2bHash;
    use std::cell::RefCell;
    use std::collections::{BTreeMap, HashMap, HashSet};
    use std::iter::once;
    use std::rc::Rc;
    use storage::global_state::inmem::*;
    use storage::history::*;
    use storage::transform::Transform;

    fn mock_tc(init_key: Key, init_account: &value::Account) -> TrackingCopy<InMemGS<Key, Value>> {
        let root_hash: Blake2bHash = [0u8; 32].into();
        let mut hist = InMemHist::new(&root_hash);
        let transform = Transform::Write(value::Value::Account(init_account.clone()));

        let mut m = HashMap::new();
        m.insert(init_key, transform);
        let commit_result = hist
            .commit(root_hash, m)
            .expect("Creation of mocked account should be a success.");

        let new_hash = match commit_result {
            CommitResult::Success(new_hash) => new_hash,
            other => panic!("Commiting changes to test History failed: {:?}.", other),
        };

        let reader = hist
            .checkout(new_hash)
            .expect("Checkout should not throw errors.")
            .expect("Root hash should exist.");

        TrackingCopy::new(reader)
    }

    fn mock_account(addr: [u8; 20]) -> (Key, value::Account) {
        let account = value::account::Account::new([0u8; 32], 0, BTreeMap::new());
        let key = Key::Account(addr);

        (key, account)
    }

    // create random account key.
    fn random_account_key<G: RngCore>(entropy_source: &mut G) -> Key {
        let mut key = [0u8; 20];
        entropy_source.fill_bytes(&mut key);
        Key::Account(key)
    }

    // create random contract key.
    fn random_contract_key<G: RngCore>(entropy_source: &mut G) -> Key {
        let mut key = [0u8; 32];
        entropy_source.fill_bytes(&mut key);
        Key::Hash(key)
    }

    // Create random URef Key.
    fn random_uref_key<G: RngCore>(entropy_source: &mut G, rights: AccessRights) -> Key {
        let mut key = [0u8; 32];
        entropy_source.fill_bytes(&mut key);
        Key::URef(key, rights)
    }

    fn mock_runtime_context<'a>(
        account: &'a Account,
        base_key: Key,
        uref_map: &'a mut BTreeMap<String, Key>,
        known_urefs: HashMap<URefAddr, HashSet<AccessRights>>,
        rng: ChaChaRng,
    ) -> RuntimeContext<'a, InMemGS<Key, Value>> {
        let tc = mock_tc(base_key, &account);
        RuntimeContext::new(
            Rc::new(RefCell::new(tc)),
            uref_map,
            known_urefs,
            Vec::new(),
            &account,
            base_key,
            0,
            0,
            0,
            rng,
        )
    }

    #[allow(clippy::assertions_on_constants)]
    fn assert_forged_reference<T>(result: Result<T, Error>) {
        match result {
            Err(Error::ForgedReference(_)) => assert!(true),
            _ => panic!("Error. Test should have failed with ForgedReference error but didn't."),
        }
    }

    #[allow(clippy::assertions_on_constants)]
    fn assert_invalid_access<T: std::fmt::Debug>(
        result: Result<T, Error>,
        expecting: AccessRights,
    ) {
        match result {
            Err(Error::InvalidAccess { required }) if required == expecting => assert!(true),
            other => panic!(
                "Error. Test should have failed with InvalidAccess error but didn't: {:?}.",
                other
            ),
        }
    }

    fn test<T, F>(
        known_urefs: HashMap<URefAddr, HashSet<AccessRights>>,
        query: F,
    ) -> Result<T, Error>
    where
        F: Fn(RuntimeContext<InMemGS<Key, Value>>) -> Result<T, Error>,
    {
        let base_acc_addr = [0u8; 20];
        let (key, account) = mock_account(base_acc_addr);
        let mut uref_map = BTreeMap::new();
        let chacha_rng = create_rng(&base_acc_addr, 0, 0);
        let runtime_context =
            mock_runtime_context(&account, key, &mut uref_map, known_urefs, chacha_rng);
        query(runtime_context)
    }

    #[test]
    fn use_uref_valid() {
        // Test fixture
        let mut rng = rand::thread_rng();
        let uref = random_uref_key(&mut rng, AccessRights::READ_WRITE);
        let known_urefs = vec_key_rights_to_map(vec![uref]);
        // Use uref as the key to perform an action on the global state.
        // This should succeed because the uref is valid.
        let query_result = test(known_urefs, |mut rc| rc.write_gs(uref, Value::Int32(43)));
        query_result.expect("writing using valid uref should succeed");
    }

    #[test]
    fn use_uref_forged() {
        // Test fixture
        let mut rng = rand::thread_rng();
        let uref = random_uref_key(&mut rng, AccessRights::READ_WRITE);
        let known_urefs = HashMap::new();
        let query_result = test(known_urefs, |mut rc| rc.write_gs(uref, Value::Int32(43)));

        assert_forged_reference(query_result);
    }

    #[test]
    fn store_contract_with_uref_valid() {
        let mut rng = rand::thread_rng();
        let uref = random_uref_key(&mut rng, AccessRights::READ_WRITE);
        let known_urefs = vec_key_rights_to_map(vec![uref]);

        let contract = Value::Contract(Contract::new(
            Vec::new(),
            once(("ValidURef".to_owned(), uref)).collect(),
        ));

        let query_result = test(known_urefs, |mut rc| {
            let contract_addr = rc
                .store_contract(contract.clone())
                .expect("Storing contract with valid URefs should succeed.");
            let contract_key = Key::Hash(contract_addr);
            rc.read_gs(&contract_key)
        });

        let contract_gs = query_result
            .expect("Reading contract from the GS should work.")
            .expect("Contract should be found.");

        assert_eq!(contract, contract_gs);
    }

    #[test]
    fn store_contract_with_uref_forged() {
        let mut rng = rand::thread_rng();
        let uref = random_uref_key(&mut rng, AccessRights::READ_WRITE);
        let contract = Value::Contract(Contract::new(
            Vec::new(),
            once(("ForgedURef".to_owned(), uref)).collect(),
        ));

        let query_result = test(HashMap::new(), |mut rc| rc.store_contract(contract.clone()));

        assert_forged_reference(query_result);
    }

    #[test]
    fn store_contract_under_uref_valid() {
        // Test that storing contract under URef that is known and has WRITE access works.
        let mut rng = rand::thread_rng();
        let contract_uref = random_uref_key(&mut rng, AccessRights::READ_WRITE);
        let known_urefs = vec_key_rights_to_map(vec![contract_uref]);
        let contract = Value::Contract(Contract::new(
            Vec::new(),
            once(("ValidURef".to_owned(), contract_uref)).collect(),
        ));

        let query_result = test(known_urefs, |mut rc| {
            rc.write_gs(contract_uref, contract.clone())
                .expect("Storing contract under known and writeable URef should work.");
            rc.read_gs(&contract_uref)
        });

        let contract_gs = query_result
            .expect("Reading contract from the GS should work.")
            .expect("Contract should be found.");

        assert_eq!(contract, contract_gs);
    }

    #[test]
    fn store_contract_under_uref_forged() {
        // Test that storing contract under URef that is not known fails with ForgedReference error.
        let mut rng = rand::thread_rng();
        let contract_uref = random_uref_key(&mut rng, AccessRights::READ_WRITE);
        let contract = Value::Contract(Contract::new(Vec::new(), BTreeMap::new()));

        let query_result = test(HashMap::new(), |mut rc| {
            rc.write_gs(contract_uref, contract.clone())
        });

        assert_forged_reference(query_result);
    }

    #[test]
    fn store_contract_uref_invalid_access() {
        // Test that storing contract under URef that is known but is not writeable fails.
        let mut rng = rand::thread_rng();
        let contract_uref = random_uref_key(&mut rng, AccessRights::READ);
        let known_urefs = vec_key_rights_to_map(vec![contract_uref]);
        let contract = Value::Contract(Contract::new(Vec::new(), BTreeMap::new()));

        let query_result = test(known_urefs, |mut rc| {
            rc.write_gs(contract_uref, contract.clone())
        });

        assert_invalid_access(query_result, AccessRights::WRITE);
    }

    #[test]
    fn account_key_not_writeable() {
        let mut rng = rand::thread_rng();
        let acc_key = random_account_key(&mut rng);
        let query_result = test(HashMap::new(), |mut rc| {
            rc.write_gs(acc_key, Value::Int32(1))
        });
        assert_invalid_access(query_result, AccessRights::WRITE);
    }

    #[test]
    fn account_key_readable_valid() {
        // Account key is readable if it is a "base" key - current context of the execution.
        let query_result = test(HashMap::new(), |mut rc| {
            let base_key = rc.base_key();
            let base_account = rc.account().clone();

            let result = rc
                .read_gs(&base_key)
                .expect("Account key is readable.")
                .expect("Account is found in GS.");

            assert_eq!(result, Value::Account(base_account.clone()));
            Ok(())
        });

        assert!(query_result.is_ok());
    }

    #[test]
    fn account_key_readable_invalid() {
        // Account key is NOT readable if it is different than the "base" key.
        let mut rng = rand::thread_rng();
        let other_acc_key = random_account_key(&mut rng);

        let query_result = test(HashMap::new(), |mut rc| rc.read_gs(&other_acc_key));

        assert_invalid_access(query_result, AccessRights::READ);
    }

    #[test]
    fn account_key_addable_valid() {
        // Account key is addable if it is a "base" key - current context of the execution.
        let mut rng = rand::thread_rng();
        let uref = random_uref_key(&mut rng, AccessRights::READ);
        let known_urefs = vec_key_rights_to_map(vec![uref]);
        let query_result = test(known_urefs, |mut rc| {
            let base_key = rc.base_key();
            let uref_name = "NewURef".to_owned();
            let named_key = Value::NamedKey(uref_name.clone(), uref);

            rc.add_gs(base_key, named_key).expect("Adding should work.");

            let named_key_transform = Transform::AddKeys(once((uref_name.clone(), uref)).collect());

            assert_eq!(*rc.effect().1.get(&base_key).unwrap(), named_key_transform);
            Ok(())
        });

        assert!(query_result.is_ok());
    }

    #[test]
    fn account_key_addable_invalid() {
        // Account key is NOT addable if it is a "base" key - current context of the execution.
        let mut rng = rand::thread_rng();
        let other_acc_key = random_account_key(&mut rng);

        let query_result = test(HashMap::new(), |mut rc| {
            rc.add_gs(other_acc_key, Value::Int32(1))
        });

        assert_invalid_access(query_result, AccessRights::ADD);
    }

    #[test]
    fn contract_key_readable_valid() {
        // Account key is readable if it is a "base" key - current context of the execution.
        let mut rng = rand::thread_rng();
        let contract_key = random_contract_key(&mut rng);
        let query_result = test(HashMap::new(), |mut rc| rc.read_gs(&contract_key));

        assert!(query_result.is_ok());
    }

    #[test]
    fn contract_key_not_writeable() {
        // Account key is readable if it is a "base" key - current context of the execution.
        let mut rng = rand::thread_rng();
        let contract_key = random_contract_key(&mut rng);
        let query_result = test(HashMap::new(), |mut rc| {
            rc.write_gs(contract_key, Value::Int32(1))
        });

        assert_invalid_access(query_result, AccessRights::WRITE);
    }

    #[test]
    fn contract_key_addable_valid() {
        // Contract key is addable if it is a "base" key - current context of the execution.
        let base_acc_addr = [0u8; 20];
        let (account_key, account) = mock_account(base_acc_addr);
        let mut rng = rand::thread_rng();
        let contract_key = random_contract_key(&mut rng);
        let contract = Value::Contract(Contract::new(Vec::new(), BTreeMap::new()));
        let tc = Rc::new(RefCell::new(mock_tc(account_key, &account)));
        // Store contract in the GlobalState so that we can mainpulate it later.
        tc.borrow_mut().write(
            Validated::new(contract_key, Validated::valid).unwrap(),
            Validated::new(contract.clone(), Validated::valid).unwrap(),
        );

        let mut uref_map = BTreeMap::new();
        let uref = random_uref_key(&mut rng, AccessRights::WRITE);
        let known_urefs = vec_key_rights_to_map(vec![uref]);
        let chacha_rng = create_rng(&base_acc_addr, 0, 0);
        let mut runtime_context = RuntimeContext::new(
            Rc::clone(&tc),
            &mut uref_map,
            known_urefs,
            Vec::new(),
            &account,
            contract_key,
            0,
            0,
            0,
            chacha_rng,
        );

        let uref_name = "NewURef".to_owned();
        let named_key = Value::NamedKey(uref_name.clone(), uref);

        runtime_context
            .add_gs(contract_key, named_key)
            .expect("Adding should work.");

        let updated_contract =
            Value::Contract(Contract::new(Vec::new(), once((uref_name, uref)).collect()));

        assert_eq!(
            *tc.borrow().effect().1.get(&contract_key).unwrap(),
            Transform::Write(updated_contract)
        );
    }

    #[test]
    fn contract_key_addable_invalid() {
        // Contract key is addable if it is a "base" key - current context of the execution.
        let base_acc_addr = [0u8; 20];
        let (account_key, account) = mock_account(base_acc_addr);
        let mut rng = rand::thread_rng();
        let contract_key = random_contract_key(&mut rng);
        let other_contract_key = random_contract_key(&mut rng);
        let contract = Value::Contract(Contract::new(Vec::new(), BTreeMap::new()));
        let tc = Rc::new(RefCell::new(mock_tc(account_key, &account)));
        // Store contract in the GlobalState so that we can mainpulate it later.
        tc.borrow_mut().write(
            Validated::new(contract_key, Validated::valid).unwrap(),
            Validated::new(contract.clone(), Validated::valid).unwrap(),
        );

        let mut uref_map = BTreeMap::new();
        let uref = random_uref_key(&mut rng, AccessRights::WRITE);
        let known_urefs = vec_key_rights_to_map(vec![uref]);
        let chacha_rng = create_rng(&base_acc_addr, 0, 0);
        let mut runtime_context = RuntimeContext::new(
            Rc::clone(&tc),
            &mut uref_map,
            known_urefs,
            Vec::new(),
            &account,
            other_contract_key,
            0,
            0,
            0,
            chacha_rng,
        );

        let uref_name = "NewURef".to_owned();
        let named_key = Value::NamedKey(uref_name.clone(), uref);

        let result = runtime_context.add_gs(contract_key, named_key);

        assert_invalid_access(result, AccessRights::ADD);
    }

    #[test]
    fn uref_key_readable_valid() {
        let mut rng = rand::thread_rng();
        let uref_key = random_uref_key(&mut rng, AccessRights::READ);
        let known_urefs = vec_key_rights_to_map(vec![uref_key]);
        let query_result = test(known_urefs, |mut rc| rc.read_gs(&uref_key));
        assert!(query_result.is_ok());
    }

    #[test]
    fn uref_key_readable_invalid() {
        let mut rng = rand::thread_rng();
        let uref_key = random_uref_key(&mut rng, AccessRights::WRITE);
        let known_urefs = vec_key_rights_to_map(vec![uref_key]);
        let query_result = test(known_urefs, |mut rc| rc.read_gs(&uref_key));
        assert_invalid_access(query_result, AccessRights::READ);
    }

    #[test]
    fn uref_key_writeable_valid() {
        let mut rng = rand::thread_rng();
        let uref_key = random_uref_key(&mut rng, AccessRights::WRITE);
        let known_urefs = vec_key_rights_to_map(vec![uref_key]);
        let query_result = test(known_urefs, |mut rc| rc.write_gs(uref_key, Value::Int32(1)));
        assert!(query_result.is_ok());
    }

    #[test]
    fn uref_key_writeable_invalid() {
        let mut rng = rand::thread_rng();
        let uref_key = random_uref_key(&mut rng, AccessRights::READ);
        let known_urefs = vec_key_rights_to_map(vec![uref_key]);
        let query_result = test(known_urefs, |mut rc| rc.write_gs(uref_key, Value::Int32(1)));
        assert_invalid_access(query_result, AccessRights::WRITE);
    }

    #[test]
    fn uref_key_addable_valid() {
        let mut rng = rand::thread_rng();
        let uref_key = random_uref_key(&mut rng, AccessRights::ADD_WRITE);
        let known_urefs = vec_key_rights_to_map(vec![uref_key]);
        let query_result = test(known_urefs, |mut rc| {
            rc.write_gs(uref_key, Value::Int32(10))
                .expect("Writing to the GlobalState should work.");
            rc.add_gs(uref_key, Value::Int32(1))
        });
        assert!(query_result.is_ok());
    }

    #[test]
    fn uref_key_addable_invalid() {
        let mut rng = rand::thread_rng();
        let uref_key = random_uref_key(&mut rng, AccessRights::WRITE);
        let known_urefs = vec_key_rights_to_map(vec![uref_key]);
        let query_result = test(known_urefs, |mut rc| rc.add_gs(uref_key, Value::Int32(1)));
        assert_invalid_access(query_result, AccessRights::ADD);
    }

    use common::bytesrepr::ToBytes;
    use execution::{instance_and_memory, Runtime};
    use parity_wasm::elements::Module;
    use wasmi::memory_units::Pages;
    use wasmi::MemoryInstance;

    #[test]
    fn verify_get_function_by_name() {
        let base_acc_addr = [0u8; 20];
        let (key, account) = mock_account(base_acc_addr);
        let mut uref_map = BTreeMap::new();
        let chacha_rng = create_rng(&base_acc_addr, 0, 0);

        let known_urefs = HashMap::new();
        let runtime_context =
            mock_runtime_context(&account, key, &mut uref_map, known_urefs, chacha_rng);

        let wat = r#"
            (module
                (func (export "add") (param i32 i32) (result i32)
                    get_local 0
                    get_local 1
                    i32.add
                )
            )
            "#;

        let parity_module: Module = {
            let wasm_binary = wabt::wat2wasm(wat).expect("failed to parse wat");
            parity_wasm::deserialize_buffer(&wasm_binary)
                .expect("Failed to deserialize bytes to Wasm module.")
        };

        // Allocate some memory
        let memory = MemoryInstance::alloc(Pages(1024), Some(Pages(1024 * 16))).expect("alloc");

        // Write an "add" string ABI-encoded
        let add_fn_data = "add".to_bytes().unwrap();
        memory.set(123, &add_fn_data).unwrap();

        let mut runtime = Runtime::new(memory, parity_module, runtime_context);

        // Lookup at offset 123
        let add_fn1 = runtime
            .get_function_by_name(123, add_fn_data.len() as u32)
            .unwrap();
        let add_fn2 = runtime
            .get_function_by_name(123, add_fn_data.len() as u32)
            .unwrap();

        assert_eq!(
            add_fn1,
            vec![
                0, 97, 115, 109, 1, 0, 0, 0, 1, 7, 1, 96, 2, 127, 127, 1, 127, 3, 2, 1, 0, 7, 8, 1,
                4, 99, 97, 108, 108, 0, 0, 10, 9, 1, 7, 0, 32, 0, 32, 1, 106, 11
            ]
        );
        assert_eq!(
            add_fn2,
            vec![
                0, 97, 115, 109, 1, 0, 0, 0, 1, 7, 1, 96, 2, 127, 127, 1, 127, 3, 2, 1, 0, 7, 8, 1,
                4, 99, 97, 108, 108, 0, 0, 10, 9, 1, 7, 0, 32, 0, 32, 1, 106, 11
            ]
        );

        // Store and retrieve contract

        let fn_bytes: Vec<u8> = (0..255).collect();
        let hash1 = runtime
            .store_function(fn_bytes.clone(), BTreeMap::new())
            .unwrap();
        let hash2 = runtime.store_function(fn_bytes, BTreeMap::new()).unwrap();
        assert_ne!(hash1, hash2);
    }
}
