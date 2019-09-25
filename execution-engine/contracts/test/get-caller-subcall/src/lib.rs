#![no_std]
#![feature(cell_update)]

extern crate alloc;
extern crate contract_ffi;
extern crate core;

use alloc::collections::btree_map::BTreeMap;
use contract_ffi::contract_api::{self, Error as ApiError};
use contract_ffi::value::account::PublicKey;
use contract_ffi::value::Value;
use core::convert::TryInto;

enum Error {
    Serialization = 1,
}

#[no_mangle]
pub extern "C" fn check_caller_ext() {
    let caller_public_key: PublicKey = contract_api::get_caller();
    let ret_value: Value = caller_public_key
        .try_into()
        .unwrap_or_else(|_| contract_api::revert(Error::Serialization as u32));
    contract_api::ret(ret_value)
}

#[no_mangle]
pub extern "C" fn call() {
    let known_public_key: PublicKey = match contract_api::get_arg(0) {
        Some(Ok(data)) => data,
        Some(Err(_)) => contract_api::revert(ApiError::InvalidArgument.into()),
        None => contract_api::revert(ApiError::MissingArgument.into()),
    };
    let caller_public_key: PublicKey = contract_api::get_caller();
    assert_eq!(
        caller_public_key, known_public_key,
        "caller public key was not known public key"
    );

    let pointer = contract_api::store_function("check_caller_ext", BTreeMap::new());
    let subcall_public_key: PublicKey = contract_api::call_contract(pointer, &());
    assert_eq!(
        subcall_public_key, known_public_key,
        "subcall public key was not known public key"
    );
}
