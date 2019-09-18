#![no_std]

extern crate contract_ffi;

use contract_ffi::contract_api;
use contract_ffi::contract_api::pointers::{ContractPointer, TURef};
use contract_ffi::key::Key;
use contract_ffi::uref::AccessRights;
use contract_ffi::value::account::PurseId;
use contract_ffi::value::{Value, U512};

enum Error {
    GetPosOuterURef = 1000,
    GetPosInnerURef = 1001,
}

fn get_pos_contract() -> ContractPointer {
    let outer: TURef<Key> = contract_api::get_uref("pos")
        .and_then(Key::to_turef)
        .unwrap_or_else(|| contract_api::revert(Error::GetPosInnerURef as u32));
    if let Some(ContractPointer::URef(inner)) = contract_api::read::<Key>(outer).to_c_ptr() {
        ContractPointer::URef(TURef::new(inner.addr(), AccessRights::READ))
    } else {
        contract_api::revert(Error::GetPosOuterURef as u32)
    }
}

const POS_BOND: &str = "bond";
const POS_UNBOND: &str = "unbond";

fn bond(pos: ContractPointer, amount: U512, source: PurseId) {
    contract_api::call_contract::<_, ()>(pos, &(POS_BOND, amount, source));
}

fn unbond(pos: ContractPointer, amount: Option<U512>) {
    contract_api::call_contract::<_, ()>(
        pos,
        &(POS_UNBOND, Value::from_serializable(amount).unwrap()),
    );
}

#[no_mangle]
pub extern "C" fn call() {
    let pos_pointer = get_pos_contract();
    let amount: U512 = contract_api::get_arg(0);
    bond(pos_pointer.clone(), amount, contract_api::main_purse());
    unbond(pos_pointer, Some(amount + 1));
}
