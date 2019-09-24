#![no_std]
#![feature(cell_update)]

extern crate alloc;
extern crate core;

extern crate contract_ffi;

use contract_ffi::contract_api::{add_associated_key, get_arg, revert, set_action_threshold};
use contract_ffi::value::account::{ActionType, PublicKey, Weight};
use contract_ffi::value::Value;
use core::convert::TryInto;

enum Error {
    AddAssociatedKey = 50,
    KeyManagementThreshold = 51,
    DeploymentThreshold = 52,
    SetKeymanagementThreshold = 100,
    SetDeploymentThreshold = 200,
}

#[no_mangle]
pub extern "C" fn call() {
    add_associated_key(PublicKey::new([123; 32]), Weight::new(254))
        .unwrap_or_else(|_| revert(Error::AddAssociatedKey as u32));
    // TODO(mpapierski): Identify additional Value variants
    let key_management_threshold: Weight = get_arg::<Value>(0)
        .try_into()
        .unwrap_or_else(|_| revert(Error::KeyManagementThreshold as u32));
    // TODO(mpapierski): Identify additional Value variants
    let deployment_threshold: Weight = get_arg::<Value>(1)
        .try_into()
        .unwrap_or_else(|_| revert(Error::DeploymentThreshold as u32));

    set_action_threshold(ActionType::KeyManagement, key_management_threshold)
        .unwrap_or_else(|_| revert(Error::SetKeymanagementThreshold as u32));
    set_action_threshold(ActionType::Deployment, deployment_threshold)
        .unwrap_or_else(|_| revert(Error::SetDeploymentThreshold as u32));
}
