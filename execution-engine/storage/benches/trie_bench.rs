#[macro_use]
extern crate criterion;
extern crate common;
extern crate shared;
extern crate storage;

use criterion::black_box;
use criterion::Criterion;

use common::bytesrepr::{FromBytes, ToBytes};
use common::key::Key;
use common::value::Value;
use shared::newtypes::Blake2bHash;
use storage::history::trie::{Pointer, PointerBlock, Trie};

fn trie_bench(c: &mut Criterion) {
    let leaf = Trie::Leaf {
        key: Key::Account([0; 20]),
        value: Value::Int32(42),
    };
    let leaf_bytes = leaf.to_bytes().unwrap();

    c.bench_function("serialize trie leaf", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&leaf)))
    });
    c.bench_function("deserialize trie leaf", move |b| {
        b.iter(|| u8::from_bytes(black_box(&leaf_bytes)))
    });

    let node = Trie::<String, String>::Node {
        pointer_block: Box::new(PointerBlock::default()),
    };
    let node_bytes = node.to_bytes().unwrap();

    c.bench_function("serialize trie node", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&node)))
    });
    c.bench_function("deserialize trie node", move |b| {
        b.iter(|| u8::from_bytes(black_box(&node_bytes)))
    });

    let node = Trie::<String, String>::Extension {
        affix: (0..255).collect(),
        pointer: Pointer::NodePointer(Blake2bHash::new(&[0; 32])),
    };
    let node_bytes = node.to_bytes().unwrap();

    c.bench_function("serialize trie node", move |b| {
        b.iter(|| ToBytes::to_bytes(black_box(&node)))
    });
    c.bench_function("deserialize trie node", move |b| {
        b.iter(|| u8::from_bytes(black_box(&node_bytes)))
    });
}

criterion_group!(benches, trie_bench,);
criterion_main!(benches);
