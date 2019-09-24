#![no_std]

extern crate contract_ffi;

use contract_ffi::contract_api;
use contract_ffi::value::uint::U512;
use contract_ffi::value::Value;

const UNBOND_METHOD_NAME: &str = "unbond";

enum Error {
    UnbondAmountTypeMismatch = 1,
    UnbondAmountSerialization = 2,
}

// Unbonding contract.
//
// Accepts unbonding amount (of type `Option<u64>`) as first argument.
// Unbonding with `None` unbonds all stakes in the PoS contract.
// Otherwise (`Some<u64>`) unbonds with part of the bonded stakes.
#[no_mangle]
pub extern "C" fn call() {
    let pos_pointer = unwrap_or_revert(contract_api::get_pos(), 77);

    let unbond_amount: Option<U512> = {
        let value: Value = contract_api::get_arg(0);
        let maybe_amount: Option<u64> = value
            .try_deserialize()
            .unwrap_or_else(|_| contract_api::revert(Error::UnbondAmountTypeMismatch as u32));
        maybe_amount.map(Into::into)
    };

    contract_api::call_contract(
        pos_pointer,
        &(
            UNBOND_METHOD_NAME,
            Value::from_serializable(unbond_amount)
                .unwrap_or_else(|_| contract_api::revert(Error::UnbondAmountSerialization as u32)),
        ),
    )
}

fn unwrap_or_revert<T>(option: Option<T>, code: u32) -> T {
    if let Some(value) = option {
        value
    } else {
        contract_api::revert(code)
    }
}
