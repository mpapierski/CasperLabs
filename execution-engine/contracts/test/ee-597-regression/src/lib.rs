#![no_std]

extern crate alloc;
extern crate contract_ffi;
use contract_ffi::contract_api;
use contract_ffi::contract_api::pointers::ContractPointer;
use contract_ffi::value::account::PurseId;
use contract_ffi::value::U512;

const POS_BOND: &str = "bond";

fn bond(pos: ContractPointer, amount: &U512, source: PurseId) {
    contract_api::call_contract::<_, ()>(pos, &(POS_BOND, *amount, source));
}

#[no_mangle]
pub extern "C" fn call() {
    bond(
        contract_api::get_pos(),
        &U512::from(0),
        contract_api::main_purse(),
    );
}
