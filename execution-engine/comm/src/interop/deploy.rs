use crate::interop::deploy_code::DeployCode;
use jni::objects::{GlobalRef, JClass, JObject, JString};

pub struct Deploy {
    address: [u8; 32],
    session: DeployCode,
    payment: DeployCode,
    /// in units of Tokens -- someday this will come from running payment code
    tokens_transferred_in_payment: u64,
    /// in units of Token / Gas
    gas_price: u64,
    nonce: u64,
    // Public keys used to sign this deploy, to be checked against the keys
    // associated with the account.
    authorization_keys: Vec<[u8; 32]>,
}
