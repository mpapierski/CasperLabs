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

#[test]
fn should_query_with_metrics() {
    setup();
    let correlation_id = CorrelationId::new();
    let mocked_account = mocked_account(test_support::MOCKED_ACCOUNT_ADDRESS);
    let global_state = InMemoryGlobalState::from_pairs(correlation_id, &mocked_account).unwrap();
    let root_hash = global_state.root_hash.to_vec();
    let engine_state = EngineState::new(global_state, false);

    let mut query_request = QueryRequest::new();
    {
        let mut key = state::Key::new();
        let mut key_address = Key_Address::new();
        key_address.set_account(test_support::MOCKED_ACCOUNT_ADDRESS.to_vec());
        key.set_address(key_address);

        query_request.set_base_key(key);
        query_request.set_path(vec![].into());
        query_request.set_state_hash(root_hash);
    }

    let _query_response_result = engine_state
        .query(RequestOptions::new(), query_request)
        .wait_drop_metadata();

    let log_items = BUFFERED_LOGGER
        .extract_correlated(&correlation_id.to_string())
        .expect("log items expected");

    for log_item in log_items {
        assert!(
            log_item
                .properties
                .contains_key(&"correlation_id".to_string()),
            "should have correlation_id"
        );

        let matched_correlation_id = log_item
            .properties
            .get(&"correlation_id".to_string())
            .expect("should have correlation id value");

        assert_eq!(
            matched_correlation_id,
            &correlation_id.to_string(),
            "correlation_id should match"
        );

        assert_eq!(log_item.log_level, "Metric", "expected Metric");
    }
}

#[test]
fn should_exec_with_metrics() {
    setup();
    let correlation_id = CorrelationId::new();
    let mocked_account = mocked_account(test_support::MOCKED_ACCOUNT_ADDRESS);
    let global_state = InMemoryGlobalState::from_pairs(correlation_id, &mocked_account).unwrap();
    let root_hash = global_state.root_hash.to_vec();
    let engine_state = EngineState::new(global_state, false);

    let mut exec_request = ExecRequest::new();
    {
        let mut deploys: protobuf::RepeatedField<Deploy> = <protobuf::RepeatedField<Deploy>>::new();
        deploys.push(test_support::get_mock_deploy());

        exec_request.set_deploys(deploys);
        exec_request.set_parent_state_hash(root_hash);
        exec_request.set_protocol_version(test_support::get_protocol_version());
    }

    let _exec_response_result = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata();

    let log_items = BUFFERED_LOGGER
        .extract_correlated(&correlation_id.to_string())
        .expect("log items expected");

    for log_item in log_items {
        assert!(
            log_item
                .properties
                .contains_key(&"correlation_id".to_string()),
            "should have correlation_id"
        );

        let matched_correlation_id = log_item
            .properties
            .get(&"correlation_id".to_string())
            .expect("should have correlation id value");

        assert_eq!(
            matched_correlation_id,
            &correlation_id.to_string(),
            "correlation_id should match"
        );

        assert_eq!(log_item.log_level, "Metric", "expected Metric");
    }
}

#[test]
fn should_commit_with_metrics() {
    setup();
    let correlation_id = CorrelationId::new();
    let mocked_account = mocked_account(test_support::MOCKED_ACCOUNT_ADDRESS);
    let global_state = InMemoryGlobalState::from_pairs(correlation_id, &mocked_account).unwrap();
    let root_hash = global_state.root_hash.to_vec();
    let engine_state = EngineState::new(global_state, false);

    let request_options = RequestOptions::new();

    let mut commit_request = CommitRequest::new();

    commit_request.set_effects(vec![].into());
    commit_request.set_prestate_hash(root_hash);

    let _commit_response_result = engine_state
        .commit(request_options, commit_request)
        .wait_drop_metadata();

    let log_items = BUFFERED_LOGGER
        .extract_correlated(&correlation_id.to_string())
        .expect("log items expected");

    for log_item in log_items {
        assert!(
            log_item
                .properties
                .contains_key(&"correlation_id".to_string()),
            "should have correlation_id"
        );

        let matched_correlation_id = log_item
            .properties
            .get(&"correlation_id".to_string())
            .expect("should have correlation id value");

        assert_eq!(
            matched_correlation_id,
            &correlation_id.to_string(),
            "correlation_id should match"
        );

        assert_eq!(log_item.log_level, "Metric", "expected Metric");
    }
}

#[test]
fn should_validate_with_metrics() {
    setup();
    let correlation_id = CorrelationId::new();
    let mocked_account = mocked_account(test_support::MOCKED_ACCOUNT_ADDRESS);
    let global_state = InMemoryGlobalState::from_pairs(correlation_id, &mocked_account).unwrap();
    let engine_state = EngineState::new(global_state, false);

    let mut validate_request = ValidateRequest::new();

    let wasm_bytes = test_utils::create_empty_wasm_module_bytes();

    validate_request.set_payment_code(wasm_bytes.clone());
    validate_request.set_session_code(wasm_bytes);

    let _validate_response_result = engine_state
        .validate(RequestOptions::new(), validate_request)
        .wait_drop_metadata();

    let log_items = BUFFERED_LOGGER
        .extract_correlated(&correlation_id.to_string())
        .expect("log items expected");

    for log_item in log_items {
        assert!(
            log_item
                .properties
                .contains_key(&"correlation_id".to_string()),
            "should have correlation_id"
        );

        let matched_correlation_id = log_item
            .properties
            .get(&"correlation_id".to_string())
            .expect("should have correlation id value");

        assert_eq!(
            matched_correlation_id,
            &correlation_id.to_string(),
            "correlation_id should match"
        );

        assert_eq!(log_item.log_level, "Metric", "expected Metric");
    }
}

#[test]
fn should_run_genesis() {
    let global_state = InMemoryGlobalState::empty().expect("should create global state");
    let engine_state = EngineState::new(global_state, false);

    let genesis_request = {
        let genesis_account_addr = [6u8; 32].to_vec();

        let initial_tokens = {
            let mut ret = BigInt::new();
            ret.set_bit_width(512);
            ret.set_value("1000000".to_string());
            ret
        };

        let mint_code = {
            let mut ret = DeployCode::new();
            let wasm_bytes = test_utils::create_empty_wasm_module_bytes();
            ret.set_code(wasm_bytes);
            ret
        };

        let proof_of_stake_code = {
            let mut ret = DeployCode::new();
            let wasm_bytes = test_utils::create_empty_wasm_module_bytes();
            ret.set_code(wasm_bytes);
            ret
        };

        let protocol_version = {
            let mut ret = ProtocolVersion::new();
            ret.set_value(1);
            ret
        };

        let mut ret = GenesisRequest::new();
        ret.set_address(genesis_account_addr.to_vec());
        ret.set_initial_tokens(initial_tokens);
        ret.set_mint_code(mint_code);
        ret.set_proof_of_stake_code(proof_of_stake_code);
        ret.set_protocol_version(protocol_version);
        ret
    };

    let request_options = RequestOptions::new();

    let genesis_response = engine_state
        .run_genesis(request_options, genesis_request)
        .wait_drop_metadata();

    let response = genesis_response.unwrap();

    let state_handle = engine_state.state();

    let state_handle_guard = state_handle.lock();

    let state_root_hash = state_handle_guard.root_hash;
    let response_root_hash = response.get_success().get_poststate_hash();

    assert_eq!(state_root_hash.to_vec(), response_root_hash.to_vec());
}

#[ignore]
#[test]
fn should_run_genesis_with_mint_bytes() {
    let global_state = InMemoryGlobalState::empty().expect("should create global state");
    let engine_state = EngineState::new(global_state, false);

    let genesis_request = test_support::create_genesis_request();

    let request_options = RequestOptions::new();

    let genesis_response = engine_state
        .run_genesis(request_options, genesis_request)
        .wait_drop_metadata();

    let response = genesis_response.unwrap();

    let state_handle = engine_state.state();

    let state_handle_guard = state_handle.lock();

    let state_root_hash = state_handle_guard.root_hash;
    let response_root_hash = response.get_success().get_poststate_hash();

    assert_eq!(state_root_hash.to_vec(), response_root_hash.to_vec());
}

#[ignore]
#[test]
fn should_run_fake_faucet() {
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

    let exec_request = test_support::create_exec_request("fake_faucet.wasm", post_state_hash);

    let log_items = BUFFERED_LOGGER
        .extract_correlated(&correlation_id.to_string())
        .expect("log items expected");

    println!("{:?}", log_items);

    let exec_response_result = engine_state
        .exec(RequestOptions::new(), exec_request)
        .wait_drop_metadata();

    println!("{:?}", exec_response_result);
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
