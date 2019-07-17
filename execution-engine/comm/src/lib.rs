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

use jni::objects::{GlobalRef, JClass, JObject, JString};
use jni::sys::{jbyteArray, jint, jlong, jobject, jobjectArray, jstring};
use jni::JNIEnv;

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

#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_exec(_env: JNIEnv, _class: JClass) -> jobject {
    unimplemented!();
}

#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_commit(_env: JNIEnv, _class: JClass) -> jstring {
    unimplemented!();
}

#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_validate(_env: JNIEnv, _class: JClass) -> jstring {
    unimplemented!();
}

#[allow(non_snake_case)]
pub extern "system" fn Java_ExecutionEngine_run_genesis(_env: JNIEnv, _class: JClass) -> jstring {
    unimplemented!();
}
