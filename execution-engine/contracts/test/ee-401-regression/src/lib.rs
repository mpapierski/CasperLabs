#![no_std]
#![feature(cell_update)]

extern crate alloc;

extern crate contract_ffi;

use alloc::collections::btree_map::BTreeMap;
use alloc::string::String;

use contract_ffi::contract_api;
use contract_ffi::contract_api::pointers::ContractPointer;
use contract_ffi::uref::URef;

#[no_mangle]
pub extern "C" fn hello_ext() {
    let test_string = String::from("Hello, world!");
    let test_uref: URef = contract_api::new_uref(test_string).into();
    contract_api::ret(test_uref)
}

#[no_mangle]
pub extern "C" fn call() {
    let known_urefs = BTreeMap::new();
    let contract_pointer: ContractPointer = contract_api::store_function("hello_ext", known_urefs);
    contract_api::add_uref("hello_ext", &contract_pointer.into());
}
