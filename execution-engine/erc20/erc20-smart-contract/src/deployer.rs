use alloc::vec;

use contract_ffi::contract_api::runtime;
use contract_ffi::contract_api::storage;
use contract_ffi::contract_api::TURef;
use contract_ffi::contract_api::ContractRef;
use contract_ffi::value::Value;
use contract_ffi::value::U512;
use contract_ffi::key::Key;

use crate::error::Error;
use crate::api::Api;

// ERC20 smart contract.
use crate::erc20::erc20;

// Proxy smart contract.
use crate::proxy::erc20_proxy;

pub fn deploy() {
    match Api::from_args() {
        Api::Deploy(name, initial_balance) => {
            deploy_token(&name, initial_balance);
            deploy_proxy();
        }
        _ => runtime::revert(Error::UnknownDeployCommand)
    }
}

fn deploy_token(name: &str, initial_balance: U512) {
    // Create erc20 token instance.
    let token_ref: ContractRef = storage::store_function_at_hash("erc20", Default::default());

    // Initialize erc20 contract.
    runtime::call_contract::<_, ()>(token_ref.clone(), &("init_erc20", initial_balance), &vec![]);

    // Save it under a new TURef.
    let token_turef: TURef<Key> = storage::new_turef(token_ref.into());

    // Save TURef under readalbe name.
    runtime::put_key(&name, &token_turef.into());
}

fn deploy_proxy() {
    // Create proxy instance.
    let proxy_ref: ContractRef = storage::store_function_at_hash("erc20_proxy", Default::default());

    // Save it under a new TURef.
    let proxy_turef: TURef<Key> = storage::new_turef(proxy_ref.into());

    // Save TURef under readalbe name.
    runtime::put_key("erc20_proxy", &proxy_turef.into());   
}

