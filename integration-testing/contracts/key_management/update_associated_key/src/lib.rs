#![no_std]
#![feature(cell_update)]

extern crate alloc;
extern crate contract_ffi;

use contract_ffi::contract_api::{get_arg, revert, update_associated_key};
use contract_ffi::value::account::{PublicKey, Weight};
use contract_ffi::value::Value;

#[no_mangle]
pub extern "C" fn call() {
    let account: PublicKey = get_arg(0);
    let weight_val: u8 = get_arg::<Value>(1).try_deserialize().unwrap();
    let weight = Weight::new(weight_val as u8);

    update_associated_key(account, weight)
        .unwrap_or_else(|_| revert(100));
}
