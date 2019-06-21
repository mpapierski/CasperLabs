#[macro_use]
extern crate lazy_static;
extern crate grpc;

extern crate casperlabs_engine_grpc_server;
extern crate common;
extern crate execution_engine;
extern crate shared;
extern crate storage;

mod test_support;

use std::convert::TryInto;

use grpc::RequestOptions;

use common::bytesrepr::ToBytes;
use common::key::Key;
use execution_engine::engine_state::EngineState;
use shared::init::mocked_account;
use shared::logging::log_level::LogLevel;
use shared::logging::log_settings::{self, LogLevelFilter, LogSettings};
use shared::logging::logger::{self, LogBufferProvider, BUFFERED_LOGGER};
use shared::newtypes::CorrelationId;
use shared::test_utils;
use shared::transform::Transform;
use storage::global_state::in_memory::InMemoryGlobalState;
use storage::global_state::History;

use casperlabs_engine_grpc_server::engine_server::ipc::{
    CommitRequest, Deploy, DeployCode, ExecRequest, ExecutionEffect, GenesisRequest, QueryRequest,
    ValidateRequest,
};
use casperlabs_engine_grpc_server::engine_server::ipc_grpc::ExecutionEngineService;
use casperlabs_engine_grpc_server::engine_server::mappings::CommitTransforms;
use casperlabs_engine_grpc_server::engine_server::state::{
    self, BigInt, Key_Address, ProtocolVersion,
};
use execution_engine::tracking_copy::TrackingCopy;

pub const PROC_NAME: &str = "ee-shared-lib-tests";


pub fn get_log_settings(log_level: LogLevel) -> LogSettings {
    let log_level_filter = LogLevelFilter::new(log_level);
    LogSettings::new(PROC_NAME, log_level_filter)
}

fn setup() {
    logger::initialize_buffered_logger();
    log_settings::set_log_settings_provider(&*LOG_SETTINGS);
}

lazy_static! {
    static ref LOG_SETTINGS: LogSettings = get_log_settings(LogLevel::Error);
}

#[ignore]
#[test]
fn should_run_local_state_contract() {
    setup();
    let correlation_id = CorrelationId::new();
    let mocked_account = mocked_account(test_support::MOCKED_ACCOUNT_ADDRESS);
    let global_state = InMemoryGlobalState::from_pairs(correlation_id, &mocked_account).unwrap();
    let engine_state = EngineState::new(global_state, false);

    let genesis_request = test_support::create_genesis_request();

    let request_options = RequestOptions::new();

    let genesis_response = engine_state
        .run_genesis(request_options, genesis_request)
        .wait_drop_metadata();

    let response = genesis_response.unwrap();

    let effect: &ExecutionEffect = response.get_success().get_effect();

    let map: CommitTransforms = effect
        .get_transform_map()
        .try_into()
        .expect("should convert");

    let map = map.value();

    let state_handle = engine_state.state();

    let state_root_hash = {
        let state_handle_guard = state_handle.lock();
        let root_hash = state_handle_guard.root_hash;
        let mut tracking_copy: TrackingCopy<InMemoryGlobalState> = state_handle_guard
            .checkout(root_hash)
            .expect("should return global state")
            .map(TrackingCopy::new)
            .expect("should return tracking copy");

        for (k, v) in map.iter() {
            if let Transform::Write(v) = v {
                assert_eq!(
                    Some(v.to_owned()),
                    tracking_copy.get(correlation_id, k).expect("should get")
                );
            } else {
                panic!("ffuuu");
            }
        }

        root_hash
    };

    let response_root_hash = response.get_success().get_poststate_hash();

    let post_state_hash = response_root_hash.to_vec();

    assert_eq!(state_root_hash.to_vec(), post_state_hash);
    println!("post state hash {:?}", post_state_hash);
    let exec_request = test_support::create_exec_request("local_state.wasm", post_state_hash);

    let log_items = BUFFERED_LOGGER
        .extract_correlated(&correlation_id.to_string())
        .expect("log items expected");

    println!("{:?}", log_items);

    let exec_response = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata()
        .expect("should exec");

    println!("{:?}", exec_response);

    assert!(exec_response.has_success());
    let execution_result = exec_response
        .get_success()
        .get_deploy_results()
        // Get first deploy result
        .get(0)
        .unwrap()
        .get_execution_result();


    let expected_local_key = Key::local([6; 32], &[66u8; 32].to_bytes().unwrap());

    // Compare op key
    let local_key_op = {
        let value = execution_result
            .get_effects()
            .get_op_map()
            .get(0)
            .unwrap()
            .get_key()
            .get_local()
            .get_hash();
        assert_eq!(value.len(), 32);
        let mut local = [0u8; 32];
        local.copy_from_slice(value);
        Key::Local(local)
    };
    assert_eq!(local_key_op, expected_local_key);


    // Compare transform effects
    let transform_map = execution_result.get_effects().get_transform_map();
    let transform_entry = transform_map.get(0).unwrap();

    let local_key = {
        let value = transform_entry.get_key().get_local().get_hash();
        assert_eq!(value.len(), 32);
        let mut local = [0u8; 32];
        local.copy_from_slice(value);
        Key::Local(local)
    };

    assert_eq!(local_key, expected_local_key);
    assert_eq!(
        transform_entry
            .get_transform()
            .get_write()
            .get_value()
            .get_string_value(),
        String::from("Hello, world!")
    );
}