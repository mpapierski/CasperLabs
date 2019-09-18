#![no_std]

extern crate contract_ffi;

use contract_ffi::contract_api;
use contract_ffi::uref::URef;
use contract_ffi::value::account::PurseId;
use contract_ffi::value::{Value, U512};

#[repr(u32)]
enum Error {
    MintNotFound = 1,
    BalanceNotFound = 2,
    BalanceMismatch = 3,
}

fn mint_purse(amount: U512) -> PurseId {
    let mint = contract_api::get_mint().expect("mint contract should exist");

    let result_uref: URef = contract_api::call_contract(mint, &("mint", amount));

    PurseId::new(result_uref)
}

#[no_mangle]
pub extern "C" fn call() {
    let amount: U512 = 12345.into();
    let new_purse = mint_purse(amount);

    let mint = contract_api::get_mint()
        .unwrap_or_else(|| contract_api::revert(Error::MintNotFound as u32));

    // TODO(mpapierski): Identify additional Value variants
    let balance: Option<U512> =
        contract_api::call_contract::<_, Value>(mint, &("balance", new_purse))
            .try_deserialize()
            .unwrap();

    match balance {
        None => contract_api::revert(Error::BalanceNotFound as u32),

        Some(balance) if balance == amount => (),

        _ => contract_api::revert(Error::BalanceMismatch as u32),
    }
}
