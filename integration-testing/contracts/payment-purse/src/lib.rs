#![no_std]

extern crate contract_ffi;

use contract_ffi::contract_api::{self, PurseTransferResult};
use contract_ffi::key::Key;
use contract_ffi::value::account::PurseId;
use contract_ffi::value::{Value, U512};

#[no_mangle]
pub extern "C" fn call() {
    let pos_contract: Key =
        contract_api::read(contract_api::get_uref("pos").unwrap().to_turef().unwrap());
    let pos_pointer = pos_contract.to_c_ptr().unwrap();

    let source_purse = contract_api::main_purse();
    let payment_amount: U512 = contract_api::get_arg::<Value>(1).try_deserialize().unwrap();
    let payment_purse: PurseId =
        contract_api::call_contract(pos_pointer, &("get_payment_purse",));

    // can deposit
    if let PurseTransferResult::TransferError =
        contract_api::transfer_from_purse_to_purse(source_purse, payment_purse, payment_amount)
    {
        contract_api::revert(1);
    }

    let payment_balance = match contract_api::get_balance(payment_purse) {
        Some(amount) => amount,
        None => contract_api::revert(2),
    };

    if payment_balance != payment_amount {
        contract_api::revert(3)
    }

    // cannot withdraw
    if let PurseTransferResult::TransferSuccessful =
        contract_api::transfer_from_purse_to_purse(payment_purse, source_purse, payment_amount)
    {
        contract_api::revert(4);
    }
}
