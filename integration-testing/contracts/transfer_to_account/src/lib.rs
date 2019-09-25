
#![no_std]
#![feature(cell_update)]

extern crate alloc;
extern crate contract_ffi;

use contract_ffi::value::account::PublicKey;
use contract_ffi::value::U512;
use contract_ffi::contract_api::{get_arg, revert, TransferResult};
use contract_ffi::value::Value;

#[no_mangle]
pub extern "C" fn call() {
    let public_key: PublicKey = get_arg(0).unwrap().unwrap();
    let transfer_amount: u32 = get_arg::<Value>(1).try_deserialize().unwrap();

    let amount = U512::from(transfer_amount);

    let result = contract_ffi::contract_api::transfer_to_account(public_key, amount);

    if result == TransferResult::TransferError {
        revert(1);
    }
}
