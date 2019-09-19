#![no_std]

extern crate contract_ffi;

use contract_ffi::contract_api;
use contract_ffi::contract_api::pointers::{ContractPointer, TURef};
use contract_ffi::contract_api::{call_contract, get_uref, read, revert};
use contract_ffi::key::Key;
use contract_ffi::uref::AccessRights;
use contract_ffi::value::{Value, U512};

enum Error {
    GetPosOuterURef = 1000,
    GetPosInnerURef = 1001,
}

fn get_pos_contract() -> ContractPointer {
    let outer: TURef<Key> = get_uref("pos")
        .and_then(Key::to_turef)
        .unwrap_or_else(|| revert(Error::GetPosInnerURef as u32));
    if let Some(ContractPointer::URef(inner)) = read::<Key>(outer).to_c_ptr() {
        ContractPointer::URef(TURef::new(inner.addr(), AccessRights::READ))
    } else {
        revert(Error::GetPosOuterURef as u32)
    }
}

#[no_mangle]
pub extern "C" fn call() {
    let pos_contract: ContractPointer = get_pos_contract();
    // I dont have any safe method to check for the existence of the args.
    // I am utilizing 0(invalid) amount to indicate no args to EE.
    let value = contract_api::get_arg::<U512>(0);
    let unbond_amount: Option<U512> = if value == U512::zero() { None } else { Some(value) };
    let _result: () = call_contract(pos_contract, &("unbond", Value::from_serializable(unbond_amount).unwrap()));
}
