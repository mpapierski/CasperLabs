use std::{ops::Deref, sync::Arc};

use engine_shared::{
    additive_map::AdditiveMap,
    newtypes::{Blake2bHash, CorrelationId},
    stored_value::StoredValue,
    transform::Transform,
};
use types::{Key, ProtocolVersion};

use crate::{
    error,
    global_state::{commit, CommitResult, StateProvider, StateReader},
    protocol_data::ProtocolData,
    protocol_data_store::lmdb::LmdbProtocolDataStore,
    store::Store,
    transaction_source::{lmdb::LmdbEnvironment, Transaction, TransactionSource},
    trie::{operations::create_hashed_empty_trie, Trie},
    trie_store::{
        lmdb::LmdbTrieStore,
        operations::{read, ReadResult},
    },
};

pub struct LmdbGlobalState {
    pub environment: Arc<LmdbEnvironment>,
    pub trie_store: Arc<LmdbTrieStore>,
    pub protocol_data_store: Arc<LmdbProtocolDataStore>,
    pub empty_root_hash: Blake2bHash,
}

/// Represents a "view" of global state at a particular root hash.
pub struct LmdbGlobalStateView {
    pub environment: Arc<LmdbEnvironment>,
    pub store: Arc<LmdbTrieStore>,
    pub root_hash: Blake2bHash,
}

impl LmdbGlobalState {
    /// Creates an empty state from an existing environment and trie_store.
    pub fn empty(
        environment: Arc<LmdbEnvironment>,
        trie_store: Arc<LmdbTrieStore>,
        protocol_data_store: Arc<LmdbProtocolDataStore>,
    ) -> Result<Self, error::Error> {
        let root_hash: Blake2bHash = {
            let (root_hash, root) = create_hashed_empty_trie::<Key, StoredValue>()?;
            loop {
                let mut txn = environment.create_read_write_txn()?;
                match trie_store.put(&mut txn, &root_hash, &root) {
                    Ok(_) => {}
                    Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                        txn.abort();
                        environment.grow_map_size()?;
                        continue;
                    }
                    Err(e) => return Err(e),
                }

                match txn.commit() {
                    Ok(_) => break,
                    Err(e) if e.is_map_full() => {
                        environment.grow_map_size()?;
                        continue;
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            root_hash
        };
        Ok(LmdbGlobalState::new(
            environment,
            trie_store,
            protocol_data_store,
            root_hash,
        ))
    }

    /// Creates a state from an existing environment, store, and root_hash.
    /// Intended to be used for testing.
    pub(crate) fn new(
        environment: Arc<LmdbEnvironment>,
        trie_store: Arc<LmdbTrieStore>,
        protocol_data_store: Arc<LmdbProtocolDataStore>,
        empty_root_hash: Blake2bHash,
    ) -> Self {
        LmdbGlobalState {
            environment,
            trie_store,
            protocol_data_store,
            empty_root_hash,
        }
    }
}

impl StateReader<Key, StoredValue> for LmdbGlobalStateView {
    type Error = error::Error;

    fn read(
        &self,
        correlation_id: CorrelationId,
        key: &Key,
    ) -> Result<Option<StoredValue>, Self::Error> {
        loop {
            let txn = self.environment.create_read_txn()?;

            let ret = match read::<Key, StoredValue, lmdb::RoTransaction, LmdbTrieStore>(
                correlation_id,
                &txn,
                self.store.deref(),
                &self.root_hash,
                key,
            ) {
                Ok(ReadResult::Found(value)) => Some(value),
                Ok(ReadResult::NotFound) => None,
                Ok(ReadResult::RootNotFound) => panic!("LmdbGlobalState has invalid root"),
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    txn.abort();
                    self.environment.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            match txn.commit() {
                Ok(_) => return Ok(ret),
                Err(e) if e.is_map_full() => {
                    self.environment.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

impl StateProvider for LmdbGlobalState {
    type Error = error::Error;

    type Reader = LmdbGlobalStateView;

    fn checkout(&self, state_hash: Blake2bHash) -> Result<Option<Self::Reader>, Self::Error> {
        loop {
            let txn = self.environment.create_read_txn()?;

            let maybe_root_res: Result<Option<Trie<Key, StoredValue>>, Self::Error> =
                self.trie_store.get(&txn, &state_hash);
            match maybe_root_res {
                Ok(maybe_root) => {
                    let maybe_state = maybe_root.map(|_| LmdbGlobalStateView {
                        environment: Arc::clone(&self.environment),
                        store: Arc::clone(&self.trie_store),
                        root_hash: state_hash,
                    });
                    match txn.commit() {
                        Ok(_) => return Ok(maybe_state),
                        Err(e) if e.is_map_full() => {
                            self.environment.grow_map_size()?;
                            continue;
                        }
                        Err(e) => return Err(e.into()),
                    }
                }
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    txn.abort();
                    self.environment.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn commit(
        &self,
        correlation_id: CorrelationId,
        prestate_hash: Blake2bHash,
        effects: AdditiveMap<Key, Transform>,
    ) -> Result<CommitResult, Self::Error> {
        loop {
            match commit::<LmdbEnvironment, LmdbTrieStore, _>(
                &self.environment,
                &self.trie_store,
                correlation_id,
                prestate_hash,
                effects.clone(),
            ) {
                Ok(result) => return Ok(result),
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    // Abort is already called when a TX object is destroyed
                    self.environment.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn put_protocol_data(
        &self,
        protocol_version: ProtocolVersion,
        protocol_data: &ProtocolData,
    ) -> Result<(), Self::Error> {
        loop {
            let mut txn = self.environment.create_read_write_txn()?;
            match self
                .protocol_data_store
                .put(&mut txn, &protocol_version, protocol_data)
            {
                Ok(_) => {}
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    txn.abort();
                    self.environment.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            }

            match txn.commit() {
                Ok(_) => return Ok(()),
                Err(e) if e.is_map_full() => {
                    self.environment.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn get_protocol_data(
        &self,
        protocol_version: ProtocolVersion,
    ) -> Result<Option<ProtocolData>, Self::Error> {
        loop {
            let txn = self.environment.create_read_txn()?;
            let result = match self.protocol_data_store.get(&txn, &protocol_version) {
                Ok(result) => result,
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    self.environment.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            };
            match txn.commit() {
                Ok(_) => return Ok(result),
                Err(e) if e.is_map_full() => {
                    self.environment.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }

    fn empty_root(&self) -> Blake2bHash {
        self.empty_root_hash
    }
}

#[cfg(test)]
mod tests {
    use lmdb::DatabaseFlags;
    use tempfile::tempdir;

    use types::{account::PublicKey, CLValue};

    use crate::{
        trie_store::operations::{write, WriteResult},
        TEST_MAP_SIZE,
    };

    use super::*;

    #[derive(Debug, Clone)]
    struct TestPair {
        key: Key,
        value: StoredValue,
    }

    fn create_test_pairs() -> [TestPair; 2] {
        [
            TestPair {
                key: Key::Account(PublicKey::ed25519_from([1_u8; 32])),
                value: StoredValue::CLValue(CLValue::from_t(1_i32).unwrap()),
            },
            TestPair {
                key: Key::Account(PublicKey::ed25519_from([2_u8; 32])),
                value: StoredValue::CLValue(CLValue::from_t(2_i32).unwrap()),
            },
        ]
    }

    fn create_test_pairs_updated() -> [TestPair; 3] {
        [
            TestPair {
                key: Key::Account(PublicKey::ed25519_from([1u8; 32])),
                value: StoredValue::CLValue(CLValue::from_t("one".to_string()).unwrap()),
            },
            TestPair {
                key: Key::Account(PublicKey::ed25519_from([2u8; 32])),
                value: StoredValue::CLValue(CLValue::from_t("two".to_string()).unwrap()),
            },
            TestPair {
                key: Key::Account(PublicKey::ed25519_from([3u8; 32])),
                value: StoredValue::CLValue(CLValue::from_t(3_i32).unwrap()),
            },
        ]
    }

    fn create_test_state() -> (LmdbGlobalState, Blake2bHash) {
        let correlation_id = CorrelationId::new();
        let _temp_dir = tempdir().unwrap();
        let environment = Arc::new(
            LmdbEnvironment::new(&_temp_dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap(),
        );
        let trie_store =
            Arc::new(LmdbTrieStore::new(&environment, None, DatabaseFlags::empty()).unwrap());
        let protocol_data_store = Arc::new(
            LmdbProtocolDataStore::new(&environment, None, DatabaseFlags::empty()).unwrap(),
        );
        let ret = LmdbGlobalState::empty(environment, trie_store, protocol_data_store).unwrap();
        let mut current_root = ret.empty_root_hash;
        {
            let mut txn = ret.environment.create_read_write_txn().unwrap();

            for TestPair { key, value } in &create_test_pairs() {
                match write(
                    correlation_id,
                    &mut txn,
                    ret.trie_store.deref(),
                    &current_root,
                    key,
                    value,
                )
                .unwrap()
                {
                    WriteResult::Written(root_hash) => {
                        current_root = root_hash;
                    }
                    WriteResult::AlreadyExists => (),
                    WriteResult::RootNotFound => panic!("LmdbGlobalState has invalid root"),
                }
            }

            txn.commit().unwrap();
        }
        (ret, current_root)
    }

    #[test]
    fn reads_from_a_checkout_return_expected_values() {
        let correlation_id = CorrelationId::new();
        let (state, root_hash) = create_test_state();
        let checkout = state.checkout(root_hash).unwrap().unwrap();
        for TestPair { key, value } in create_test_pairs().iter().cloned() {
            assert_eq!(Some(value), checkout.read(correlation_id, &key).unwrap());
        }
    }

    #[test]
    fn checkout_fails_if_unknown_hash_is_given() {
        let (state, _) = create_test_state();
        let fake_hash: Blake2bHash = [1u8; 32].into();
        let result = state.checkout(fake_hash).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn commit_updates_state() {
        let correlation_id = CorrelationId::new();
        let test_pairs_updated = create_test_pairs_updated();

        let (state, root_hash) = create_test_state();

        let effects: AdditiveMap<Key, Transform> = {
            let mut tmp = AdditiveMap::new();
            for TestPair { key, value } in &test_pairs_updated {
                tmp.insert(*key, Transform::Write(value.to_owned()));
            }
            tmp
        };

        let updated_hash = match state.commit(correlation_id, root_hash, effects).unwrap() {
            CommitResult::Success { state_root, .. } => state_root,
            _ => panic!("commit failed"),
        };

        let updated_checkout = state.checkout(updated_hash).unwrap().unwrap();

        for TestPair { key, value } in test_pairs_updated.iter().cloned() {
            assert_eq!(
                Some(value),
                updated_checkout.read(correlation_id, &key).unwrap()
            );
        }
    }

    #[test]
    fn commit_updates_state_and_original_state_stays_intact() {
        let correlation_id = CorrelationId::new();
        let test_pairs_updated = create_test_pairs_updated();

        let (state, root_hash) = create_test_state();

        let effects: AdditiveMap<Key, Transform> = {
            let mut tmp = AdditiveMap::new();
            for TestPair { key, value } in &test_pairs_updated {
                tmp.insert(*key, Transform::Write(value.to_owned()));
            }
            tmp
        };

        let updated_hash = match state.commit(correlation_id, root_hash, effects).unwrap() {
            CommitResult::Success { state_root, .. } => state_root,
            _ => panic!("commit failed"),
        };

        let updated_checkout = state.checkout(updated_hash).unwrap().unwrap();
        for TestPair { key, value } in test_pairs_updated.iter().cloned() {
            assert_eq!(
                Some(value),
                updated_checkout.read(correlation_id, &key).unwrap()
            );
        }

        let original_checkout = state.checkout(root_hash).unwrap().unwrap();
        for TestPair { key, value } in create_test_pairs().iter().cloned() {
            assert_eq!(
                Some(value),
                original_checkout.read(correlation_id, &key).unwrap()
            );
        }
        assert_eq!(
            None,
            original_checkout
                .read(correlation_id, &test_pairs_updated[2].key)
                .unwrap()
        );
    }
}
