extern crate grpc;

extern crate casperlabs_engine_grpc_server;
extern crate common;
extern crate execution_engine;
extern crate shared;
extern crate storage;

#[allow(dead_code)]
mod test_support;

use common::bytesrepr::ToBytes;
use common::key::Key;
use common::value::Value;
use shared::transform::Transform;

const GENESIS_ADDR: [u8; 32] = [6u8; 32];

#[ignore]
#[test]
fn should_run_local_state_contract() {
    let transforms = test_support::WasmTestBuilder::new("local_state.wasm")
        .with_genesis_addr(GENESIS_ADDR)
        .expect_transforms();

    let expected_local_key = Key::local(GENESIS_ADDR, &[66u8; 32].to_bytes().unwrap());

    assert_eq!(
        transforms
            .get(&expected_local_key)
            .expect("Should have expected local key"),
        &Transform::Write(Value::String(String::from("Hello, world!")))
    );
}
