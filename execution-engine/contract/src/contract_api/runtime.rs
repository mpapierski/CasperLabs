//! Functions for interacting with the current runtime.

// Can be removed once https://github.com/rust-lang/rustfmt/issues/3362 is resolved.
#[rustfmt::skip]
use alloc::vec;
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::mem::MaybeUninit;

use casperlabs_types::{
    account::PublicKey,
    api_error,
    bytesrepr::{self, FromBytes},
    ApiError, BlockTime, CLTyped, CLValue, ContractRef, Key, Phase, URef,
    BLOCKTIME_SERIALIZED_LENGTH, PHASE_SERIALIZED_LENGTH,
};

use crate::{args_parser::ArgsParser, contract_api, ext_ffi, unwrap_or_revert::UnwrapOrRevert};

/// Returns the given [`CLValue`] to the host, terminating the currently running module.
///
/// Note this function is only relevant to contracts stored on chain which are invoked via
/// [`call_contract`] and can thus return a value to their caller.  The return value of a directly
/// deployed contract is never used.
pub fn ret(value: CLValue) -> ! {
    let (ptr, size, _bytes) = contract_api::to_ptr(value);
    unsafe {
        ext_ffi::ret(ptr, size);
    }
}

/// Stops execution of a contract and reverts execution effects with a given [`ApiError`].
///
/// The provided `ApiError` is returned in the form of a numeric exit code to the caller via the
/// deploy response.
pub fn revert<T: Into<ApiError>>(error: T) -> ! {
    unsafe {
        ext_ffi::revert(error.into().into());
    }
}

/// Calls the given stored contract, passing the given arguments to it.
///
/// If the stored contract calls [`ret`], then that value is returned from `call_contract`.  If the
/// stored contract calls [`revert`], then execution stops and `call_contract` doesn't return.
/// Otherwise `call_contract` returns `()`.
#[allow(clippy::ptr_arg)]
pub fn call_contract<A: ArgsParser, T: CLTyped + FromBytes>(c_ptr: ContractRef, args: A) -> T {
    let contract_key: Key = c_ptr.into();
    let (key_ptr, key_size, _bytes1) = contract_api::to_ptr(contract_key);
    let (args_ptr, args_size, _bytes2) = ArgsParser::parse(args)
        .map(contract_api::to_ptr)
        .unwrap_or_revert();

    let bytes_written = {
        let mut bytes_written = MaybeUninit::uninit();
        let ret = unsafe {
            ext_ffi::call_contract(
                key_ptr,
                key_size,
                args_ptr,
                args_size,
                bytes_written.as_mut_ptr(),
            )
        };
        api_error::result_from(ret).unwrap_or_revert();
        unsafe { bytes_written.assume_init() }
    };

    let serialized_result = if bytes_written == 0 {
        // If no bytes were written, the host buffer hasn't been set and hence shouldn't be read.
        vec![]
    } else {
        // NOTE: this is a copy of the contents of `read_host_buffer()`.  Calling that directly from
        // here causes several contracts to fail with a Wasmi `Unreachable` error.
        let bytes_ptr = contract_api::alloc_bytes(bytes_written);
        let mut dest: Vec<u8> =
            unsafe { Vec::from_raw_parts(bytes_ptr, bytes_written, bytes_written) };
        read_host_buffer_into(&mut dest).unwrap_or_revert();
        dest
    };

    bytesrepr::deserialize(serialized_result).unwrap_or_revert()
}

/// Takes the name of a (non-mangled) `extern "C"` function to store as a contract under the given
/// [`URef`] which should already reference a stored contract.
///
/// If successful, this overwrites the value under `uref` with a new contract instance containing
/// the original contract's named_keys, the current protocol version, and the newly created bytes of
/// the stored function.
pub fn upgrade_contract_at_uref(name: &str, uref: URef) {
    let (name_ptr, name_size, _bytes) = contract_api::to_ptr(name);
    let key: Key = uref.into();
    let (key_ptr, key_size, _bytes) = contract_api::to_ptr(key);
    let result_value =
        unsafe { ext_ffi::upgrade_contract_at_uref(name_ptr, name_size, key_ptr, key_size) };
    match api_error::result_from(result_value) {
        Ok(()) => (),
        Err(error) => revert(error),
    }
}

fn get_arg_size(i: u32) -> Option<usize> {
    let mut arg_size: usize = 0;
    let ret = unsafe { ext_ffi::get_arg_size(i as usize, &mut arg_size as *mut usize) };
    match api_error::result_from(ret) {
        Ok(_) => Some(arg_size),
        Err(ApiError::MissingArgument) => None,
        Err(e) => revert(e),
    }
}

/// Returns the i-th argument passed to the host for the current module invocation.
///
/// Note that this is only relevant to contracts stored on-chain since a contract deployed directly
/// is not invoked with any arguments.
pub fn get_arg<T: FromBytes>(i: u32) -> Option<Result<T, bytesrepr::Error>> {
    let arg_size = get_arg_size(i)?;

    let arg_bytes = {
        let res = {
            let data_ptr = contract_api::alloc_bytes(arg_size);
            let ret = unsafe { ext_ffi::get_arg(i as usize, data_ptr, arg_size) };
            let data = unsafe { Vec::from_raw_parts(data_ptr, arg_size, arg_size) };
            api_error::result_from(ret).map(|_| data)
        };
        // Assumed to be safe as `get_arg_size` checks the argument already
        res.unwrap_or_revert()
    };
    Some(bytesrepr::deserialize(arg_bytes))
}

/// Returns the caller of the current context, i.e. the [`PublicKey`] of the account which made the
/// deploy request.
pub fn get_caller() -> PublicKey {
    let output_size = {
        let mut output_size = MaybeUninit::uninit();
        let ret = unsafe { ext_ffi::get_caller(output_size.as_mut_ptr()) };
        api_error::result_from(ret).unwrap_or_revert();
        unsafe { output_size.assume_init() }
    };
    let buf = read_host_buffer(output_size).unwrap_or_revert();
    bytesrepr::deserialize(buf).unwrap_or_revert()
}

/// Returns the current [`BlockTime`].
pub fn get_blocktime() -> BlockTime {
    let dest_ptr = contract_api::alloc_bytes(BLOCKTIME_SERIALIZED_LENGTH);
    let bytes = unsafe {
        ext_ffi::get_blocktime(dest_ptr);
        Vec::from_raw_parts(
            dest_ptr,
            BLOCKTIME_SERIALIZED_LENGTH,
            BLOCKTIME_SERIALIZED_LENGTH,
        )
    };
    bytesrepr::deserialize(bytes).unwrap_or_revert()
}

/// Returns the current [`Phase`].
pub fn get_phase() -> Phase {
    let dest_ptr = contract_api::alloc_bytes(PHASE_SERIALIZED_LENGTH);
    unsafe { ext_ffi::get_phase(dest_ptr) };
    let bytes =
        unsafe { Vec::from_raw_parts(dest_ptr, PHASE_SERIALIZED_LENGTH, PHASE_SERIALIZED_LENGTH) };
    bytesrepr::deserialize(bytes).unwrap_or_revert()
}

/// Returns the requested named [`Key`] from the current context.
///
/// The current context is either the caller's account or a stored contract depending on whether the
/// currently-executing module is a direct call or a sub-call respectively.
pub fn get_key(name: &str) -> Option<Key> {
    let (name_ptr, name_size, _bytes) = contract_api::to_ptr(name);
    let mut key_bytes = vec![0u8; Key::max_serialized_length()];
    let mut total_bytes: usize = 0;
    let ret = unsafe {
        ext_ffi::get_key(
            name_ptr,
            name_size,
            key_bytes.as_mut_ptr(),
            key_bytes.len(),
            &mut total_bytes as *mut usize,
        )
    };
    match api_error::result_from(ret) {
        Ok(_) => {}
        Err(ApiError::MissingKey) => return None,
        Err(e) => revert(e),
    }
    key_bytes.truncate(total_bytes);
    let key: Key = bytesrepr::deserialize(key_bytes).unwrap_or_revert();
    Some(key)
}

/// Returns `true` if `name` exists in the current context's named keys.
///
/// The current context is either the caller's account or a stored contract depending on whether the
/// currently-executing module is a direct call or a sub-call respectively.
pub fn has_key(name: &str) -> bool {
    let (name_ptr, name_size, _bytes) = contract_api::to_ptr(name);
    let result = unsafe { ext_ffi::has_key(name_ptr, name_size) };
    result == 0
}

/// Stores the given [`Key`] under `name` in the current context's named keys.
///
/// The current context is either the caller's account or a stored contract depending on whether the
/// currently-executing module is a direct call or a sub-call respectively.
pub fn put_key(name: &str, key: Key) {
    let (name_ptr, name_size, _bytes) = contract_api::to_ptr(name);
    let (key_ptr, key_size, _bytes2) = contract_api::to_ptr(key);
    unsafe { ext_ffi::put_key(name_ptr, name_size, key_ptr, key_size) };
}

/// Removes the [`Key`] stored under `name` in the current context's named keys.
///
/// The current context is either the caller's account or a stored contract depending on whether the
/// currently-executing module is a direct call or a sub-call respectively.
pub fn remove_key(name: &str) {
    let (name_ptr, name_size, _bytes) = contract_api::to_ptr(name);
    unsafe { ext_ffi::remove_key(name_ptr, name_size) }
}

/// Returns the named keys of the current context.
///
/// The current context is either the caller's account or a stored contract depending on whether the
/// currently-executing module is a direct call or a sub-call respectively.
pub fn list_named_keys() -> BTreeMap<String, Key> {
    let (total_keys, result_size) = {
        let mut total_keys = MaybeUninit::uninit();
        let mut result_size = 0;
        let ret = unsafe {
            ext_ffi::load_named_keys(total_keys.as_mut_ptr(), &mut result_size as *mut usize)
        };
        api_error::result_from(ret).unwrap_or_revert();
        let total_keys = unsafe { total_keys.assume_init() };
        (total_keys, result_size)
    };
    if total_keys == 0 {
        return BTreeMap::new();
    }
    let bytes = read_host_buffer(result_size).unwrap_or_revert();
    bytesrepr::deserialize(bytes).unwrap_or_revert()
}

/// Validates uref against named keys.
pub fn is_valid_uref(uref: URef) -> bool {
    let (uref_ptr, uref_size, _bytes) = contract_api::to_ptr(uref);
    let result = unsafe { ext_ffi::is_valid_uref(uref_ptr, uref_size) };
    result != 0
}

fn read_host_buffer_into(dest: &mut [u8]) -> Result<usize, ApiError> {
    let mut bytes_written = MaybeUninit::uninit();
    let ret = unsafe {
        ext_ffi::read_host_buffer(dest.as_mut_ptr(), dest.len(), bytes_written.as_mut_ptr())
    };
    // NOTE: When rewriting below expression as `result_from(ret).map(|_| unsafe { ... })`, and the
    // caller ignores the return value, execution of the contract becomes unstable and ultimately
    // leads to `Unreachable` error.
    api_error::result_from(ret)?;
    Ok(unsafe { bytes_written.assume_init() })
}

pub(crate) fn read_host_buffer(size: usize) -> Result<Vec<u8>, ApiError> {
    let bytes_ptr = contract_api::alloc_bytes(size);
    let mut dest: Vec<u8> = unsafe { Vec::from_raw_parts(bytes_ptr, size, size) };
    read_host_buffer_into(&mut dest)?;
    Ok(dest)
}
