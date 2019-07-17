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
use std::time::Instant;
use std::sync::Arc;
use std::path::PathBuf;

use execution_engine::execution::WasmiExecutor;
use jni::objects::{GlobalRef, JClass, JObject, JString};
use jni::sys::{jbyteArray, jint, jlong, jobject, jobjectArray, jstring};
use jni::JNIEnv;
use shared::newtypes::Blake2bHash;
use wasm_prep::wasm_costs::WasmCosts;
use wasm_prep::WasmiPreprocessor;
use lmdb::DatabaseFlags;
use storage::global_state::lmdb::LmdbGlobalState;
use storage::trie_store::lmdb::{LmdbEnvironment, LmdbTrieStore};
use execution_engine::engine_state::EngineState;
use shared::os::get_page_size;
use std::fs;


fn get_engine_state(data_dir: PathBuf, map_size: usize) -> EngineState<LmdbGlobalState> {
    if let Err(err) = fs::create_dir_all(&data_dir) {
        eprintln!("Get engine state create dir {:?}: {:?}", data_dir, err);
    }

    let environment = {
        let ret = LmdbEnvironment::new(&data_dir, map_size).expect("should create lmdb environment");
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
    // map_size: jlong,
) {

    // let path = {
    //         let mut dir = home_dir().expect("should get home dir");
    //         dir.push(".casperlabs");
    //         dir
    //     }

    let data_dir: String = env.get_string(data_dir).expect("should get string").into();

    let map_size = get_page_size().expect("should get page size");
    let engine_state = get_engine_state(data_dir.into(), map_size * 4);
    // let environment = Arc::new(LmdbEnvironment::new(&data_dir.into(), map_size).expect("should create lmdb environment"));

    // let trie_store = Arc::new(LmdbTrieStore::new(&environment, None, DatabaseFlags::empty())
    //         .expect("should create lmdb trie store"));

    // let global_state = LmdbGlobalState::empty(environment, trie_store)
    //     .expect("should create empty lmdb store");

    // let engine_state = EngineState::new(global_state);


    // let new_object = env.find_class("java/lang/Object").expect("should find class");
    // let allocated_object = env.alloc_object(new_object).expect("should alloc object");

    env.set_rust_field(context, "rustPrivPtr", engine_state).expect("should set rust field");

    println!("init: success");
    // allocated_object.into_inner()
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_destroy(
    env: JNIEnv,
    class: JClass,
    context: JObject,
) {
    let _engine_state: EngineState<LmdbGlobalState> = env.take_rust_field(context, "rustPrivPtr").expect("should take engine_state field");
    println!("destroy: success");
}



#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_exec(
    env: JNIEnv,
    _class: JClass,
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

    //     let blocktime = BlockTime(exec_request.get_block_time());

    // TODO: don't unwrap
    // TODO: JNI seems to expose signed types only - check if its safe
    let wasm_costs =
        WasmCosts::from_version(protocol_version as u64).expect("should create wasm costs");

    let preprocessor: WasmiPreprocessor = WasmiPreprocessor::new(wasm_costs);

    let executor = WasmiExecutor;

    // TODO: don't unwrap
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
