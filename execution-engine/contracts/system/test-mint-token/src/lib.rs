#![no_std]

extern crate alloc;
extern crate contract_ffi;

use alloc::string::String;

use contract_ffi::contract_api;
use contract_ffi::key::Key;
use contract_ffi::value::{Value, U512};

#[no_mangle]
pub extern "C" fn call() {
    let mint = Key::Hash([
        164, 102, 153, 51, 236, 214, 169, 167, 126, 44, 250, 247, 179, 214, 203, 229, 239, 69, 145,
        25, 5, 153, 113, 55, 255, 188, 176, 201, 7, 4, 42, 100,
    ])
    .to_c_ptr()
    .unwrap();
    //let x = contract_api::get_uref("mint");

    let amount1 = U512::from(100);
    let purse1: Key = contract_api::call_contract(mint.clone(), &("create", amount1));

    let amount2 = U512::from(300);
    let purse2: Key = contract_api::call_contract(mint.clone(), &("create", amount2));

    let result: String =
        contract_api::call_contract(mint.clone(), &("transfer", purse1, purse2, U512::from(70)));

    assert!(&result == "Success!");

    // TODO(mpapierski): Identify new Value variants
    let new_amount1: Option<U512> =
        contract_api::call_contract::<_, Value>(mint.clone(), &("balance", purse1))
            .try_deserialize()
            .unwrap();
    let new_amount2: Option<U512> =
        contract_api::call_contract::<_, Value>(mint.clone(), &("balance", purse2))
            .try_deserialize()
            .unwrap();

    assert!(new_amount1.unwrap() == U512::from(30));
    assert!(new_amount2.unwrap() == U512::from(370));
}
