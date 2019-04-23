use crate::key::*;
use crate::value::*;
use alloc::collections::BTreeMap;
use alloc::string::String;
use proptest::collection::{btree_map, vec};
use proptest::prelude::*;

pub fn u8_slice_20() -> impl Strategy<Value = [u8; 20]> {
    vec(any::<u8>(), 20).prop_map(|b| {
        let mut res = [0u8; 20];
        res.clone_from_slice(b.as_slice());
        res
    })
}

pub fn u8_slice_32() -> impl Strategy<Value = [u8; 32]> {
    vec(any::<u8>(), 32).prop_map(|b| {
        let mut res = [0u8; 32];
        res.clone_from_slice(b.as_slice());
        res
    })
}

pub fn uref_map_arb(depth: usize) -> impl Strategy<Value = BTreeMap<String, Key>> {
    btree_map("\\PC*", key_arb(), depth)
}

pub fn access_rights_arb() -> impl Strategy<Value = AccessRights> {
    prop_oneof![
        Just(AccessRights::READ),
        Just(AccessRights::ADD),
        Just(AccessRights::WRITE),
        Just(AccessRights::READ_ADD),
        Just(AccessRights::READ_WRITE),
        Just(AccessRights::ADD_WRITE),
        Just(AccessRights::READ_ADD_WRITE),
    ]
}

pub fn key_arb() -> impl Strategy<Value = Key> {
    prop_oneof![
        u8_slice_20().prop_map(Key::Account),
        u8_slice_32().prop_map(Key::Hash),
        access_rights_arb()
            .prop_flat_map(|right| { u8_slice_32().prop_map(move |addr| Key::URef(addr, right)) })
    ]
}

pub fn account_arb() -> impl Strategy<Value = Account> {
    u8_slice_32().prop_flat_map(|b| {
        any::<u64>().prop_flat_map(move |u64arb| {
            uref_map_arb(3).prop_map(move |urefs| Account::new(b, u64arb, urefs))
        })
    })
}

pub fn contract_arb() -> impl Strategy<Value = Contract> {
    uref_map_arb(20).prop_flat_map(|urefs| {
        vec(any::<u8>(), 1..1000).prop_map(move |body| Contract::new(body, urefs.clone()))
    })
}

pub fn u128_arb() -> impl Strategy<Value = U128> {
    vec(any::<u8>(), 0..16).prop_map(|b| U128::from_little_endian(b.as_slice()))
}

pub fn u256_arb() -> impl Strategy<Value = U256> {
    vec(any::<u8>(), 0..32).prop_map(|b| U256::from_little_endian(b.as_slice()))
}

pub fn u512_arb() -> impl Strategy<Value = U512> {
    vec(any::<u8>(), 0..64).prop_map(|b| U512::from_little_endian(b.as_slice()))
}

pub fn value_arb() -> impl Strategy<Value = Value> {
    prop_oneof![
        (any::<i32>().prop_map(Value::Int32)),
        (vec(any::<u8>(), 1..1000).prop_map(Value::ByteArray)),
        (vec(any::<i32>(), 1..1000).prop_map(Value::ListInt32)),
        ("\\PC*".prop_map(Value::String)),
        (vec(any::<String>(), 1..500).prop_map(Value::ListString)),
        ("\\PC*", key_arb()).prop_map(|(n, k)| Value::NamedKey(n, k)),
        account_arb().prop_map(Value::Account),
        contract_arb().prop_map(Value::Contract),
        u128_arb().prop_map(Value::UInt128),
        u256_arb().prop_map(Value::UInt256),
        u512_arb().prop_map(Value::UInt512)
    ]
}
