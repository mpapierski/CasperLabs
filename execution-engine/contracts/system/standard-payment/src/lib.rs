#![no_std]

use casperlabs_standard_payment::{
    AccountProvider, MintProvider, ProofOfStakeProvider, StandardPayment,
};
use contract::{
    contract_api::{account, runtime, system},
    unwrap_or_revert::UnwrapOrRevert,
};
use types::{ApiError, URef, U512};

const GET_PAYMENT_PURSE: &str = "get_payment_purse";

struct StandardPaymentContract;

impl AccountProvider for StandardPaymentContract {
    fn get_main_purse(&self) -> Result<URef, ApiError> {
        Ok(account::get_main_purse())
    }
}

impl MintProvider for StandardPaymentContract {
    fn transfer_purse_to_purse(
        &mut self,
        source: URef,
        target: URef,
        amount: U512,
    ) -> Result<(), ApiError> {
        system::transfer_from_purse_to_purse(source, target, amount)
    }
}

impl ProofOfStakeProvider for StandardPaymentContract {
    fn get_payment_purse(&mut self) -> Result<URef, ApiError> {
        let pos_pointer = system::get_proof_of_stake();
        let payment_purse = runtime::call_contract(pos_pointer, (GET_PAYMENT_PURSE,));
        Ok(payment_purse)
    }
}

impl StandardPayment for StandardPaymentContract {}

pub fn delegate() {
    let mut standard_payment_contract = StandardPaymentContract;

    let amount: U512 = runtime::get_arg(0)
        .unwrap_or_revert_with(ApiError::MissingArgument)
        .unwrap_or_revert_with(ApiError::InvalidArgument);

    standard_payment_contract.pay(amount).unwrap_or_revert();
}

#[cfg(not(feature = "lib"))]
#[no_mangle]
pub extern "C" fn call() {
    delegate();
}
