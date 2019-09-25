#![no_std]

extern crate contract_ffi;

use contract_ffi::contract_api::{self, Error as ApiError};
use contract_ffi::uref::URef;
use contract_ffi::value::account::PurseId;
use contract_ffi::value::U512;

#[repr(u16)]
enum Error {
    BalanceNotFound = 0,
    BalanceMismatch,
}

fn mint_purse(amount: U512) -> PurseId {
    let mint = contract_api::get_mint();

    let result_uref: URef = contract_api::call_contract(mint, &("mint", amount));

    PurseId::new(result_uref)
}

#[no_mangle]
pub extern "C" fn call() {
    let amount: U512 = 12345.into();
    let new_purse = mint_purse(amount);

    let mint = contract_api::get_mint();

    // TODO(mpapierski): Identify additional Value variants
    let balance: Option<U512> =
        contract_api::call_contract(mint, &("balance", new_purse));

    match balance {
        None => contract_api::revert(ApiError::User(Error::BalanceNotFound as u16).into()),

        Some(balance) if balance == amount => (),

        _ => contract_api::revert(ApiError::User(Error::BalanceMismatch as u16).into()),
    }
}
