extern crate common;
extern crate execution_engine;
extern crate grpc;
extern crate lmdb;
extern crate proptest;
extern crate protobuf;
extern crate shared;
extern crate storage;
extern crate wabt;
extern crate wasm_prep;

#[cfg(test)]
extern crate parity_wasm;

extern crate jni;

pub mod interop;

use std::convert::TryInto;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use execution_engine::engine_state::EngineState;
use execution_engine::execution::WasmiExecutor;
use jni::objects::{GlobalRef, JClass, JObject, JString};
use jni::sys::{jbyteArray, jint, jlong, jobject, jobjectArray, jstring};
use jni::JNIEnv;
use lmdb::DatabaseFlags;
use shared::newtypes::Blake2bHash;
use shared::os::get_page_size;
use std::fs;
use storage::global_state::lmdb::LmdbGlobalState;
use storage::trie_store::lmdb::{LmdbEnvironment, LmdbTrieStore};
use wasm_prep::wasm_costs::WasmCosts;
use wasm_prep::WasmiPreprocessor;

fn get_engine_state(data_dir: PathBuf, map_size: usize) -> EngineState<LmdbGlobalState> {
    if let Err(e) = fs::create_dir_all(&data_dir) {
        if e.kind() != ErrorKind::AlreadyExists {
            eprintln!("Unable to create data dir: {:?}", e)
        }
    }

    let environment = {
        let ret =
            LmdbEnvironment::new(&data_dir, map_size).expect("should create lmdb environment");
        Arc::new(ret)
    };

    let trie_store = {
        let ret = LmdbTrieStore::new(&environment, None, DatabaseFlags::empty())
            .expect("should create lmdb trie store");
        Arc::new(ret)
    };

    let global_state = LmdbGlobalState::empty(Arc::clone(&environment), Arc::clone(&trie_store))
        .expect("should create empty lmdb global state");

    EngineState::new(global_state)
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_init(
    env: JNIEnv,
    class: JClass,
    context: JObject,
    data_dir: JString,
) {
    let data_dir: String = env.get_string(data_dir).expect("should get string").into();
    let map_size = get_page_size().expect("should get page size");
    let engine_state = get_engine_state(data_dir.into(), map_size * 4);

    // Overwrite `rustPrivPtr` attribute on passed Object as our engine state object
    // therefore giving up all Rust safety guarantees.
    env.set_rust_field(context, "rustPrivPtr", engine_state)
        .expect("should set rust field");
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_destroy(env: JNIEnv, class: JClass, context: JObject) {
    // Take back engine state object from the context object
    let _engine_state: EngineState<LmdbGlobalState> = env
        .take_rust_field(context, "rustPrivPtr")
        .expect("should take engine_state field");
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_exec(
    env: JNIEnv,
    _class: JClass,
    context: JObject,
    parent_state_hash: jbyteArray,
    block_time: jlong,
    deploys: jobjectArray,
    protocol_version: jlong,
) -> jobject {
    let start = Instant::now();

    let prestate_hash: Blake2bHash = {
        let bytes = env
            .convert_byte_array(parent_state_hash)
            .expect("should convert byte array");
        bytes
            .as_slice()
            .try_into()
            .expect("should create blake2bash")
    };

    let wasm_costs =
        WasmCosts::from_version(protocol_version as u64).expect("should create wasm costs");

    let preprocessor: WasmiPreprocessor = WasmiPreprocessor::new(wasm_costs);

    let executor = WasmiExecutor;

    for i in 0..env.get_array_length(deploys).unwrap() {
        let _deploy_jobject = env
            .get_object_array_element(deploys, i)
            .expect("should get object array element");
    }
    unimplemented!();
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_commit(_env: JNIEnv, _class: JClass) -> jstring {
    unimplemented!();
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_query(
    _env: JNIEnv,
    _class: JClass,
    // bytes state_hash = 1;
    // io.casperlabs.casper.consensus.state.Key base_key = 2;
    // repeated string path = 3;
) -> jobject {
    unimplemented!();
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_validate(_env: JNIEnv, _class: JClass) -> jstring {
    unimplemented!();
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_run_genesis(_env: JNIEnv, _class: JClass) -> jstring {
    unimplemented!();
}
