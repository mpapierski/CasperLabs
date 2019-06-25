extern crate grpc;

extern crate casperlabs_engine_grpc_server;
extern crate common;
extern crate execution_engine;
extern crate shared;
extern crate storage;

#[allow(dead_code)]
mod test_support;

// use common::bytesrepr::ToBytes;
use common::key::Key;
use common::value::{Value, U512};
use shared::transform::Transform;

const GENESIS_ADDR: [u8; 32] = [6u8; 32];

#[ignore]
#[test]
fn should_run_known_urefs_contract() {
    let transforms = test_support::WasmTestBuilder::new("known_urefs.wasm")
        .with_genesis_addr(GENESIS_ADDR)
        .expect_transforms();

    // Execution yields 3 transformations 2 of which are urefs
    assert_eq!(transforms.len(), 3);

    let s = transforms
        .iter()
        .filter_map(|(k, v)| {
            if let Transform::Write(Value::String(s)) = v {
                if let Key::URef(_) = k {
                    return Some(s);
                }
            }
            None
        })
        .nth(0)
        .expect("Should have write string");
    assert_eq!(s, "Hello, world!");

    let u512_value = transforms
        .iter()
        .filter_map(|(k, v)| {
            if let Transform::Write(Value::UInt512(value)) = v {
                if let Key::URef(_) = k {
                    return Some(value);
                }
            }
            None
        })
        .nth(0)
        .expect("Should have write string");

    assert_eq!(u512_value, &U512::from(123456789u64));

    let account = transforms
        .get(&Key::Account(GENESIS_ADDR))
        .and_then(|transform| {
            if let Transform::Write(Value::Account(account)) = transform {
                Some(account)
            } else {
                None
            }
        })
        .expect("Unable to get account transformation");
    // Those named URefs are created, although removed at the end of the test
    assert!(account.urefs_lookup().get("URef1").is_none());
    assert!(account.urefs_lookup().get("URef2").is_none());
}
