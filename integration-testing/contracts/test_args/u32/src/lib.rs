#![no_std]
#![feature(cell_update)]

extern crate alloc;
extern crate contract_ffi;

use contract_ffi::contract_api::{get_arg, revert};
use contract_ffi::value::U512;

#[no_mangle]
pub extern "C" fn call() {
    let number: U512 = get_arg(0).unwrap().unwrap();
    revert(number.as_u32());
}
