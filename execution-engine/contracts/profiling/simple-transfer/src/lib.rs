#![no_std]

extern crate contract_ffi;

use contract_ffi::contract_api::{self, TransferResult};
use contract_ffi::value::account::PublicKey;
use contract_ffi::value::U512;

enum Arg {
    PublicKey = 0,
    Amount = 1,
}

enum Error {
    NonExistentAccount = 1,
    Transfer = 2,
    MissingArgument = 100,
    InvalidArgument = 101,
}

#[no_mangle]
pub extern "C" fn call() {
    let public_key: PublicKey = contract_api::get_arg(Arg::PublicKey as u32)
        .unwrap_or_else(|| contract_api::revert(Error::MissingArgument as u32))
        .unwrap_or_else(|_| contract_api::revert(Error::InvalidArgument as u32));
    let amount: U512 = contract_api::get_arg(Arg::Amount as u32)
        .unwrap_or_else(|| contract_api::revert(Error::MissingArgument as u32))
        .unwrap_or_else(|_| contract_api::revert(Error::InvalidArgument as u32));
    match contract_api::transfer_to_account(public_key, amount) {
        TransferResult::TransferredToNewAccount => {
            contract_api::revert(Error::NonExistentAccount as u32)
        }
        TransferResult::TransferredToExistingAccount => (),
        TransferResult::TransferError => contract_api::revert(Error::Transfer as u32),
    }
}