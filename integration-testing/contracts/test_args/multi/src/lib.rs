#![no_std]
#![feature(cell_update)]

extern crate alloc;
extern crate contract_ffi;

use contract_ffi::contract_api::{get_arg, revert};
use contract_ffi::value::{PublicKey, U512};

#[no_mangle]
pub extern "C" fn call() {
    let account_number: PublicKey = get_arg(0);
    let number: U512 = get_arg(1);

    let account_sum : U512 = account_number.value().into_iter().map(|&value| U512::from(value)).fold(U512::zero(), |sum, val| sum + val);
    let total_sum = account_sum + number;

    revert(total_sum.as_u32());
}
