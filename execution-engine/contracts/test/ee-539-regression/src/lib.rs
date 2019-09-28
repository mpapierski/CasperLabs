#![no_std]
#![feature(cell_update)]

extern crate alloc;
extern crate core;

extern crate contract_ffi;

use contract_ffi::contract_api::{
    add_associated_key, get_arg, revert, set_action_threshold, Error as ApiError,
};
use contract_ffi::value::account::{ActionType, PublicKey, Weight};

#[repr(u16)]
enum Error {
    AddAssociatedKey = 50,
    SetKeymanagementThreshold = 100,
    SetDeploymentThreshold = 200,
}

impl From<Error> for ApiError {
    fn from(value: Error) -> ApiError {
        ApiError::User(value as u16)
    }
}

#[no_mangle]
pub extern "C" fn call() {
    add_associated_key(PublicKey::new([123; 32]), Weight::new(254))
        .unwrap_or_else(|_| revert(ApiError::from(Error::AddAssociatedKey).into()));
    let key_management_threshold: Weight = match get_arg(0) {
        Some(Ok(data)) => data,
        Some(Err(_)) => revert(ApiError::InvalidArgument.into()),
        None => revert(ApiError::MissingArgument.into()),
    };
    let deployment_threshold: Weight = match get_arg(1) {
        Some(Ok(data)) => data,
        Some(Err(_)) => revert(ApiError::InvalidArgument.into()),
        None => revert(ApiError::MissingArgument.into()),
    };

    set_action_threshold(ActionType::KeyManagement, key_management_threshold)
        .unwrap_or_else(|_| revert(ApiError::from(Error::SetKeymanagementThreshold).into()));
    set_action_threshold(ActionType::Deployment, deployment_threshold)
        .unwrap_or_else(|_| revert(ApiError::from(Error::SetDeploymentThreshold).into()));
}
