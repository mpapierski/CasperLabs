use alloc::vec::Vec;
use core::fmt::Debug;
use core::u8;

use super::error::Error;
use super::runtime::{self, revert};
use super::{alloc_bytes, to_ptr, ContractRef, TURef};
use crate::bytesrepr::deserialize;
use crate::contract_api::error::result_from;
use crate::ext_ffi;
use crate::key::Key;
use crate::system_contracts::{mint, SystemContract};
use crate::unwrap_or_revert::UnwrapOrRevert;
use crate::uref::{URef, UREF_SIZE_SERIALIZED};
use crate::value::account::{PublicKey, PurseId, PURSE_ID_SIZE_SERIALIZED};
use crate::value::U512;

pub type TransferResult = Result<TransferredTo, Error>;

pub const MINT_NAME: &str = "mint";
pub const POS_NAME: &str = "pos";

fn get_system_contract(system_contract: SystemContract) -> ContractRef {
    let system_contract_index = system_contract.into();
    let uref = {
        let result = {
            let mut uref_data_raw = [0u8; UREF_SIZE_SERIALIZED];
            let value = unsafe {
                ext_ffi::get_system_contract(
                    system_contract_index,
                    uref_data_raw.as_mut_ptr(),
                    uref_data_raw.len(),
                )
            };
            result_from(value).map(|_| uref_data_raw)
        };
        // Revert for any possible error that happened on host side
        let uref_bytes = result.unwrap_or_else(|e| revert(e));
        // Deserializes a valid URef passed from the host side
        deserialize(&uref_bytes).unwrap_or_revert()
    };
    let reference = TURef::from_uref(uref).unwrap_or_else(|_| revert(Error::NoAccessRights));
    ContractRef::TURef(reference)
}

/// Returns a read-only pointer to the Mint Contract.  Any failure will trigger `revert()` with a
/// `contract_api::Error`.
pub fn get_mint() -> ContractRef {
    get_system_contract(SystemContract::Mint)
}

/// Returns a read-only pointer to the Proof of Stake Contract.  Any failure will trigger `revert()`
/// with a `contract_api::Error`.
pub fn get_proof_of_stake() -> ContractRef {
    get_system_contract(SystemContract::ProofOfStake)
}

pub fn create_purse() -> PurseId {
    let purse_id_ptr = alloc_bytes(PURSE_ID_SIZE_SERIALIZED);
    unsafe {
        let ret = ext_ffi::create_purse(purse_id_ptr, PURSE_ID_SIZE_SERIALIZED);
        if ret == 0 {
            let bytes = Vec::from_raw_parts(
                purse_id_ptr,
                PURSE_ID_SIZE_SERIALIZED,
                PURSE_ID_SIZE_SERIALIZED,
            );
            deserialize(&bytes).unwrap_or_revert()
        } else {
            panic!("could not create purse_id")
        }
    }
}

/// Gets the balance of a given purse
pub fn get_balance(purse_id: PurseId) -> Option<U512> {
    let (purse_id_ptr, purse_id_size, _bytes) = to_ptr(&purse_id);

    let balance_bytes: Vec<u8> = unsafe {
        let value_size = ext_ffi::get_balance(purse_id_ptr, purse_id_size) as usize;
        if value_size == 0 {
            return None;
        }
        let dest_ptr = alloc_bytes(value_size);
        ext_ffi::get_read(dest_ptr);
        Vec::from_raw_parts(dest_ptr, value_size, value_size)
    };

    let balance: U512 = deserialize(&balance_bytes).unwrap_or_revert();

    Some(balance)
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(i32)]
pub enum TransferredTo {
    ExistingAccount = 0,
    NewAccount = 1,
}

impl TransferredTo {
    fn result_from(value: i32) -> TransferResult {
        match value {
            x if x == TransferredTo::ExistingAccount as i32 => Ok(TransferredTo::ExistingAccount),
            x if x == TransferredTo::NewAccount as i32 => Ok(TransferredTo::NewAccount),
            _ => Err(Error::Transfer),
        }
    }

    pub fn i32_from(result: TransferResult) -> i32 {
        match result {
            Ok(transferred_to) => transferred_to as i32,
            Err(_) => 2,
        }
    }
}

/// Transfers `amount` of motes from default purse of the account to `target`
/// account. If `target` does not exist it will create it.
pub fn transfer_to_account(target: PublicKey, amount: U512) -> TransferResult {
    let (target_ptr, target_size, _bytes) = to_ptr(&target);
    let (amount_ptr, amount_size, _bytes) = to_ptr(&amount);
    let return_code =
        unsafe { ext_ffi::transfer_to_account(target_ptr, target_size, amount_ptr, amount_size) };
    TransferredTo::result_from(return_code)
}

/// Transfers `amount` of motes from `source` purse to `target` account.
/// If `target` does not exist it will create it.
pub fn transfer_from_purse_to_account(
    source: PurseId,
    target: PublicKey,
    amount: U512,
) -> TransferResult {
    let (source_ptr, source_size, _bytes) = to_ptr(&source);
    let (target_ptr, target_size, _bytes) = to_ptr(&target);
    let (amount_ptr, amount_size, _bytes) = to_ptr(&amount);
    let return_code = unsafe {
        ext_ffi::transfer_from_purse_to_account(
            source_ptr,
            source_size,
            target_ptr,
            target_size,
            amount_ptr,
            amount_size,
        )
    };
    TransferredTo::result_from(return_code)
}

/// Transfers `amount` of motes from `source` purse to `target` purse.
pub fn transfer_from_purse_to_purse(
    source: PurseId,
    target: PurseId,
    amount: U512,
) -> Result<(), Error> {
    let mint_contract_ref = get_mint();

    let source_value: URef = source.value();
    let target_value: URef = target.value();

    let args = ("transfer", source_value, target_value, amount);

    let extra_urefs: Vec<Key> = vec![source_value.into(), target_value.into()];

    let mint_result: Result<(), mint::Error> =
        runtime::call_contract(mint_contract_ref, &args, &extra_urefs);

    mint_result.map_err(|_| Error::Transfer)
}
