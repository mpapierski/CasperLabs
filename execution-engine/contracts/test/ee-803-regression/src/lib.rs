#![no_std]

extern crate alloc;

// Can be removed once https://github.com/rust-lang/rustfmt/issues/3362 is resolved.
#[rustfmt::skip]
use alloc::vec;
use alloc::{string::String, vec::Vec};

use contract_ffi::{
    contract_api::{runtime, system, ContractRef, Error as ApiError},
    key::Key,
    unwrap_or_revert::UnwrapOrRevert,
    value::{account::PurseId, U512},
};

const POS_BOND: &str = "bond";
const POS_UNBOND: &str = "unbond";

const COMMAND_BOND: &str = "bond";
const COMMAND_UNBOND: &str = "unbond";

fn purse_to_key(p: PurseId) -> Key {
    Key::URef(p.value())
}

fn bond(pos: &ContractRef, amount: &U512, source: PurseId) {
    runtime::call_contract::<_, ()>(
        pos.clone(),
        &(POS_BOND, *amount, source),
        &vec![purse_to_key(source)],
    );
}

fn unbond(pos: &ContractRef, amount: Option<U512>) {
    runtime::call_contract::<_, ()>(pos.clone(), &(POS_UNBOND, amount), &Vec::<Key>::new());
}

#[no_mangle]
pub extern "C" fn call() {
    let command: String = runtime::get_arg(0)
        .unwrap_or_revert_with(ApiError::MissingArgument)
        .unwrap_or_revert_with(ApiError::InvalidArgument);
    let pos_pointer = system::get_proof_of_stake();
    if command == COMMAND_BOND {
        let rewards_purse = runtime::get_arg(1)
            .unwrap_or_revert_with(ApiError::MissingArgument)
            .unwrap_or_revert_with(ApiError::InvalidArgument);
        //let available_reward = system::get_balance(rewards_purse).unwrap_or_revert();
        // Attempt to bond using the rewards purse - should not be possible
        bond(&pos_pointer, &U512::from(100), rewards_purse);
    } else if command == COMMAND_UNBOND {
        unbond(&pos_pointer, None);
    }
}
