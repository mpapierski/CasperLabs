#![no_std]

extern crate contract_ffi;
use contract_ffi::contract_api::pointers::ContractPointer;
use contract_ffi::contract_api::{self, Error};
use contract_ffi::value::account::PurseId;
use contract_ffi::value::{Value, U512};

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
    let pos_pointer = contract_api::get_pos();
    let amount: U512 = match contract_api::get_arg(0) {
        Some(Ok(data)) => data,
        Some(Err(_)) => contract_api::revert(Error::InvalidArgument.into()),
        None => contract_api::revert(Error::MissingArgument.into()),
    };
    bond(pos_pointer.clone(), amount, contract_api::main_purse());
    unbond(pos_pointer, Some(amount + 1));
}
