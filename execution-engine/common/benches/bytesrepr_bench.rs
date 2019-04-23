#[macro_use]
extern crate criterion;
extern crate casperlabs_contract_ffi;

use std::collections::BTreeMap;
use std::iter;

use criterion::black_box;
use criterion::BatchSize;
use criterion::Criterion;
use criterion::ParameterizedBenchmark;

use casperlabs_contract_ffi::bytesrepr::{FromBytes, ToBytes};
use casperlabs_contract_ffi::key::{AccessRights, Key};
use casperlabs_contract_ffi::value::{
    account::Account,
    contract::Contract,
    uint::{U128, U256, U512},
    Value,
};

static KB: usize = 1024;

fn prepare_vector(size: usize) -> Vec<i32> {
    (0..size as i32).collect()
}

fn bytesrepr_bench(c: &mut Criterion) {
    let batch = 4 * KB;

    c.bench_function("serialize vector of i32s", move |b| {
        b.iter_batched_ref(
            || prepare_vector(black_box(batch)),
            |data| data.to_bytes().unwrap(),
            BatchSize::SmallInput,
        )
    });

    let data: Vec<u8> = prepare_vector(batch).to_bytes().unwrap();

    c.bench_function("deserialize vector of i32s", move |b| {
        b.iter_batched_ref(
            || data.clone(),
            |data| {
                let (res, _rem): (Vec<i32>, _) = FromBytes::from_bytes(data).unwrap();
                res
            },
            BatchSize::SmallInput,
        )
    });

    // 0, 1, ... 254, 255, 0, 1, ...
    let raw_bytes: Vec<u8> = prepare_vector(batch)
        .into_iter()
        .map(|value| value as u8)
        .collect::<Vec<_>>()
        .to_bytes()
        .unwrap();
    let data = raw_bytes.clone();

    c.bench_function("serialize vector of u8", move |b| {
        b.iter(|| data.to_bytes())
    });

    let data: Vec<u8> = prepare_vector(batch).to_bytes().unwrap();

    c.bench_function("deserialize vector of u8s", move |b| {
        b.iter(|| Vec::<i32>::from_bytes(&data))
    });

    c.bench_function("serialize u8", |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&129u8)))
    });
    c.bench_function("deserialize u8", |b| {
        b.iter(|| u8::from_bytes(black_box(&[129u8])))
    });

    c.bench_function("serialize i32", |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&1816142132i32)))
    });
    c.bench_function("deserialize i32", |b| {
        b.iter(|| i32::from_bytes(black_box(&[0x34, 0x21, 0x40, 0x6c])))
    });

    c.bench_function("serialize u64", |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&14157907845468752670u64)))
    });
    c.bench_function("deserialize u64", |b| {
        b.iter(|| u64::from_bytes(black_box(&[0x1e, 0x8b, 0xe1, 0x73, 0x2c, 0xfe, 0x7a, 0xc4])))
    });

    c.bench_function("serialize u64", |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&14157907845468752670u64)))
    });
    c.bench_function("deserialize u64", |b| {
        b.iter(|| u64::from_bytes(black_box(&[0x1e, 0x8b, 0xe1, 0x73, 0x2c, 0xfe, 0x7a, 0xc4])))
    });

    let data = Some(14157907845468752670u64);
    c.bench_function("serialize ok(u64)", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&data)))
    });
    let data = data.to_bytes().unwrap();
    c.bench_function("deserialize ok(u64)", move |b| {
        b.iter(|| Option::<u64>::from_bytes(&data))
    });

    let data: Option<u64> = None;
    c.bench_function("serialize none(u64)", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&data)))
    });
    let data = data.to_bytes().unwrap();
    c.bench_function("deserialize ok(u64)", move |b| {
        b.iter(|| Option::<u64>::from_bytes(&data))
    });

    let raw_data: Vec<Vec<u8>> = (0..4)
        .map(|_v| {
            // 0, 1, 2, ..., 254, 255
            iter::repeat_with(|| 0..255u8)
                .flatten()
                // 4 times to create 4x 1024 bytes
                .take(4)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<Vec<_>>>();

    let data = raw_data.clone();
    c.bench_function("serialize vector of vector of u8", move |b| {
        b.iter_batched_ref(
            || data.clone(),
            |data| data.to_bytes().unwrap(),
            BatchSize::SmallInput,
        )
    });

    let data = raw_data.clone().to_bytes().unwrap();
    c.bench_function("deserialize vector fo vector of u8", move |b| {
        b.iter(|| Vec::<Vec<u8>>::from_bytes(&data))
    });

    let data = {
        let mut res = BTreeMap::new();
        res.insert("asdf".to_string(), "zxcv".to_string());
        res.insert("qwer".to_string(), "rewq".to_string());
        res.insert("1234".to_string(), "5678".to_string());
        res
    };
    c.bench_function("serialize tree map", |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&14157907845468752670u64)))
    });

    let data = data.clone().to_bytes().unwrap();
    c.bench_function("deserialize tree map", move |b| {
        b.iter(|| BTreeMap::<String, String>::from_bytes(black_box(&data)))
    });

    let lorem = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua.";
    let data = lorem.to_string();
    c.bench_function("serialize string", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&data)))
    });

    let data = lorem.to_bytes().unwrap();
    c.bench_function("deserialize string", move |b| {
        b.iter(|| String::from_bytes(&data))
    });

    let array_of_lorem: Vec<String> = lorem.split(" ").map(Into::into).collect();
    let data = array_of_lorem.clone();
    c.bench_function("serialize vec of string", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&data)))
    });

    let data = array_of_lorem.to_bytes().unwrap();
    c.bench_function("deserialize vec of string", move |b| {
        b.iter(|| Vec::<String>::from_bytes(&data))
    });

    c.bench_function("serialize unit", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&())))
    });

    let data = ().to_bytes().unwrap();
    c.bench_function("deserialize unit", move |b| {
        b.iter(|| <()>::from_bytes(&data))
    });
}

fn key_bench(c: &mut Criterion) {
    let account = Key::Account([0u8; 20]);
    c.bench_function("serialize key account", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&account)))
    });
    let account_bytes = account.to_bytes().unwrap();
    c.bench_function("deserialize key account", move |b| {
        b.iter(|| Key::from_bytes(black_box(&account_bytes)))
    });

    let hash = Key::Hash([0u8; 32]);
    c.bench_function("serialize key hash", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&hash)))
    });
    let hash_bytes = hash.to_bytes().unwrap();
    c.bench_function("deserialize key hash", move |b| {
        b.iter(|| Key::from_bytes(black_box(&hash_bytes)))
    });

    let uref = Key::URef([0u8; 32], AccessRights::ADD_WRITE);
    c.bench_function("serialize key uref", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&uref)))
    });
    let uref_bytes = uref.to_bytes().unwrap();
    c.bench_function("deserialize key uref", move |b| {
        b.iter(|| Key::from_bytes(black_box(&uref_bytes)))
    });

    let keys: Vec<Key> = (0..32)
        .map(|i| Key::URef([i; 32], AccessRights::ADD_WRITE))
        .collect();
    let keys_bytes = keys.clone().to_bytes().unwrap();

    c.bench_function("serialize vec of keys", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&keys)))
    });
    c.bench_function("deserialize vec of keys", move |b| {
        b.iter(|| Vec::<Key>::from_bytes(black_box(&keys_bytes)))
    });

    let permissions = vec![
        AccessRights::READ,
        AccessRights::WRITE,
        AccessRights::ADD,
        AccessRights::READ_ADD,
        AccessRights::READ_WRITE,
        AccessRights::ADD_WRITE,
    ];
    c.bench(
        "access rights",
        ParameterizedBenchmark::new(
            "serialize",
            |b, elems| b.iter(|| elems.to_bytes()),
            permissions,
        ),
    );
}

fn make_known_urefs() -> BTreeMap<String, Key> {
    let mut urefs = BTreeMap::new();
    urefs.insert("ref1".to_string(), Key::URef([0u8; 32], AccessRights::READ));
    urefs.insert(
        "ref2".to_string(),
        Key::URef([1u8; 32], AccessRights::WRITE),
    );
    urefs.insert("ref3".to_string(), Key::URef([2u8; 32], AccessRights::ADD));
    urefs
}

fn make_contract() -> Contract {
    let known_urefs = make_known_urefs();
    Contract::new(vec![0u8; 1024], known_urefs)
}

fn make_account() -> Account {
    let known_urefs = make_known_urefs();
    Account::new([0u8; 32], 2635333365164409670u64, known_urefs)
}

fn account_bench(c: &mut Criterion) {
    let account = make_account();
    let account_bytes = account.clone().to_bytes().unwrap();

    c.bench_function("serialize account", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&account)))
    });

    c.bench_function("deserialize account", move |b| {
        b.iter(|| Account::from_bytes(black_box(&account_bytes)))
    });
}

fn value_bench(c: &mut Criterion) {
    let values = vec![
        Value::Int32(123456789i32),
        Value::UInt128(123456789u128.into()),
        Value::UInt256(123456789u64.into()),
        Value::UInt512(12345679u64.into()),
        Value::ByteArray((0..255).collect()),
        Value::ListInt32((0..1024).collect()),
        Value::String("Hello, world!".to_string()),
        Value::ListString(vec!["Hello".to_string(), "World".to_string()]),
        Value::NamedKey("Key".to_string(), Key::Account([0xffu8; 20])),
        Value::Account(make_account()),
        Value::Contract(make_contract()),
    ];
    let values_serialized: Vec<Vec<u8>> = values
        .clone()
        .into_iter()
        .map(|value| value.to_bytes().unwrap())
        .collect();

    c.bench(
        "value",
        ParameterizedBenchmark::new("serialize", |b, elems| b.iter(|| elems.to_bytes()), values),
    );

    c.bench(
        "value",
        ParameterizedBenchmark::new(
            "deserialize",
            |b, elems| b.iter(|| Value::from_bytes(elems)),
            values_serialized,
        ),
    );
}

fn contract_bench(c: &mut Criterion) {
    let contract = make_contract();
    let contract_bytes = contract.clone().to_bytes().unwrap();

    c.bench_function("serialize contract", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&contract)))
    });

    c.bench_function("deserialize contract", move |b| {
        b.iter(|| Contract::from_bytes(black_box(&contract_bytes)))
    });
}

fn uint_bench(c: &mut Criterion) {
    let num_u128 = U128::default();
    let num_u128_bytes = num_u128.to_bytes().unwrap();

    let num_u256 = U256::default();
    let num_u256_bytes = num_u256.to_bytes().unwrap();

    let num_u512 = U512::default();
    let num_u512_bytes = num_u512.to_bytes().unwrap();

    c.bench_function("serialize u128", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&num_u128)))
    });

    c.bench_function("deserialize u128", move |b| {
        b.iter(|| U128::from_bytes(black_box(&num_u128_bytes)))
    });

    c.bench_function("serialize u256", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&num_u256)))
    });

    c.bench_function("deserialize u256", move |b| {
        b.iter(|| U256::from_bytes(black_box(&num_u256_bytes)))
    });

    c.bench_function("serialize u512", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&num_u512)))
    });

    c.bench_function("deserialize u512", move |b| {
        b.iter(|| U512::from_bytes(black_box(&num_u512_bytes)))
    });
}

criterion_group!(
    benches,
    bytesrepr_bench,
    key_bench,
    account_bench,
    value_bench,
    contract_bench,
    uint_bench
);
criterion_main!(benches);
