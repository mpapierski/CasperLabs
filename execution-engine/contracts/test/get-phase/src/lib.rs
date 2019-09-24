#![no_std]

extern crate contract_ffi;
extern crate core;

use contract_ffi::contract_api;
use contract_ffi::execution::Phase;
use contract_ffi::value::Value;
use core::convert::TryInto;

#[no_mangle]
pub extern "C" fn call() {
    // TODO(mpapierski): Identify additional Value variants
    let known_phase: Phase = contract_api::get_arg::<Value>(0).try_into().unwrap();
    let get_phase = contract_api::get_phase();
    assert_eq!(
        get_phase, known_phase,
        "get_phase did not return known_phase"
    );
}
