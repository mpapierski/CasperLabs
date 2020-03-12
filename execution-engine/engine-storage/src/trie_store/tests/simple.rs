use std::fmt;

use lmdb::DatabaseFlags;
use tempfile::tempdir;

use types::bytesrepr::{FromBytes, ToBytes};

use super::TestData;
use crate::{
    error,
    store::StoreExt,
    transaction_source::{
        in_memory::InMemoryEnvironment, lmdb::LmdbEnvironment, Transaction, TransactionSource,
    },
    trie::Trie,
    trie_store::{in_memory::InMemoryTrieStore, lmdb::LmdbTrieStore, TrieStore},
    TEST_MAP_SIZE,
};

fn put_succeeds<'a, K, V, S, X>(
    store: &S,
    transaction_source: &'a X,
    items: &[TestData<K, V>],
) -> Result<(), error::Error>
where
    K: ToBytes,
    V: ToBytes,
    S: TrieStore<K, V>,
    X: TransactionSource<'a, Handle = S::Handle>,
    error::Error: From<X::Error>,
{
    loop {
        let mut txn = transaction_source.create_read_write_txn()?;
        let items = items.iter().map(Into::into);
        match store.put_many(&mut txn, items) {
            Ok(_) => {}
            Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                txn.abort();
                transaction_source.grow_map_size()?;
                continue;
            }
            Err(e) => return Err(e),
        }
        txn.commit()?;
        break;
    }
    Ok(())
}

#[test]
fn in_memory_put_succeeds() {
    let env = InMemoryEnvironment::new();
    let store = InMemoryTrieStore::new(&env, None);
    let data = &super::create_data()[0..1];

    assert!(put_succeeds(&store, &env, data).is_ok());
}

#[test]
fn lmdb_put_succeeds() {
    let tmp_dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&tmp_dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();
    let store = LmdbTrieStore::new(&env, None, DatabaseFlags::empty()).unwrap();
    let data = &super::create_data()[0..1];

    assert!(put_succeeds(&store, &env, data).is_ok());

    tmp_dir.close().unwrap();
}

fn put_get_succeeds<'a, K, V, S, X>(
    store: &S,
    transaction_source: &'a X,
    items: &[TestData<K, V>],
) -> Result<Vec<Option<Trie<K, V>>>, error::Error>
where
    K: ToBytes + FromBytes,
    V: ToBytes + FromBytes,
    S: TrieStore<K, V>,
    X: TransactionSource<'a, Handle = S::Handle>,
    error::Error: From<X::Error>,
{
    loop {
        let mut txn = transaction_source.create_read_write_txn()?;

        let pairs = items.iter().map(Into::into);
        match store.put_many(&mut txn, pairs) {
            Ok(_) => {}
            Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                txn.abort();
                transaction_source.grow_map_size()?;
                continue;
            }
            Err(e) => return Err(e),
        }

        let keys = items.iter().map(Into::into).map(|(k, _)| k);
        let ret = store.get_many(&txn, keys)?;
        txn.commit()?;
        return Ok(ret);
    }
}

#[test]
fn in_memory_put_get_succeeds() {
    let env = InMemoryEnvironment::new();
    let store = InMemoryTrieStore::new(&env, None);
    let data = &super::create_data()[0..1];

    let expected: Vec<Trie<Vec<u8>, Vec<u8>>> =
        data.to_vec().into_iter().map(|TestData(_, v)| v).collect();

    assert_eq!(
        expected,
        put_get_succeeds(&store, &env, data)
            .expect("put_get_succeeds failed")
            .into_iter()
            .collect::<Option<Vec<Trie<Vec<u8>, Vec<u8>>>>>()
            .expect("one of the outputs was empty")
    )
}

#[test]
fn lmdb_put_get_succeeds() {
    let tmp_dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&tmp_dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();
    let store = LmdbTrieStore::new(&env, None, DatabaseFlags::empty()).unwrap();
    let data = &super::create_data()[0..1];

    let expected: Vec<Trie<Vec<u8>, Vec<u8>>> =
        data.to_vec().into_iter().map(|TestData(_, v)| v).collect();

    assert_eq!(
        expected,
        put_get_succeeds(&store, &env, data)
            .expect("put_get_succeeds failed")
            .into_iter()
            .collect::<Option<Vec<Trie<Vec<u8>, Vec<u8>>>>>()
            .expect("one of the outputs was empty")
    );

    tmp_dir.close().unwrap();
}

#[test]
fn in_memory_put_get_many_succeeds() {
    let env = InMemoryEnvironment::new();
    let store = InMemoryTrieStore::new(&env, None);
    let data = super::create_data();

    let expected: Vec<Trie<Vec<u8>, Vec<u8>>> =
        data.to_vec().into_iter().map(|TestData(_, v)| v).collect();

    assert_eq!(
        expected,
        put_get_succeeds(&store, &env, &data)
            .expect("put_get failed")
            .into_iter()
            .collect::<Option<Vec<Trie<Vec<u8>, Vec<u8>>>>>()
            .expect("one of the outputs was empty")
    )
}

#[test]
fn lmdb_put_get_many_succeeds() {
    let tmp_dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&tmp_dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();
    let store = LmdbTrieStore::new(&env, None, DatabaseFlags::empty()).unwrap();
    let data = super::create_data();

    let expected: Vec<Trie<Vec<u8>, Vec<u8>>> =
        data.to_vec().into_iter().map(|TestData(_, v)| v).collect();

    assert_eq!(
        expected,
        put_get_succeeds(&store, &env, &data)
            .expect("put_get failed")
            .into_iter()
            .collect::<Option<Vec<Trie<Vec<u8>, Vec<u8>>>>>()
            .expect("one of the outputs was empty")
    );

    tmp_dir.close().unwrap();
}

fn uncommitted_read_write_txn_does_not_persist<'a, K, V, S, X>(
    store: &S,
    transaction_source: &'a X,
    items: &[TestData<K, V>],
) -> Result<Vec<Option<Trie<K, V>>>, error::Error>
where
    K: ToBytes + FromBytes,
    V: ToBytes + FromBytes,
    S: TrieStore<K, V>,
    X: TransactionSource<'a, Handle = S::Handle>,
    error::Error: From<X::Error>,
{
    {
        loop {
            let mut txn = transaction_source.create_read_write_txn()?;
            let items = items.iter().map(Into::into);
            match store.put_many(&mut txn, items) {
                Ok(_) => {
                    // Don't commit
                    break;
                }
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    txn.abort();
                    transaction_source.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }
    {
        let txn = transaction_source.create_read_txn()?;
        let keys = items.iter().map(|TestData(k, _)| k);
        let ret = store.get_many(&txn, keys)?;
        txn.commit()?;
        Ok(ret)
    }
}

#[test]
fn in_memory_uncommitted_read_write_txn_does_not_persist() {
    let env = InMemoryEnvironment::new();
    let store = InMemoryTrieStore::new(&env, None);
    let data = super::create_data();

    assert_eq!(
        None,
        uncommitted_read_write_txn_does_not_persist(&store, &env, &data)
            .expect("uncommitted_read_write_txn_does_not_persist failed")
            .into_iter()
            .collect::<Option<Vec<Trie<Vec<u8>, Vec<u8>>>>>()
    )
}

#[test]
fn lmdb_uncommitted_read_write_txn_does_not_persist() {
    let tmp_dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&tmp_dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();
    let store = LmdbTrieStore::new(&env, None, DatabaseFlags::empty()).unwrap();
    let data = super::create_data();

    assert_eq!(
        None,
        uncommitted_read_write_txn_does_not_persist(&store, &env, &data)
            .expect("uncommitted_read_write_txn_does_not_persist failed")
            .into_iter()
            .collect::<Option<Vec<Trie<Vec<u8>, Vec<u8>>>>>()
    );

    tmp_dir.close().unwrap();
}

fn read_write_transaction_does_not_block_read_transaction<'a, X>(
    transaction_source: &'a X,
) -> Result<(), error::Error>
where
    X: TransactionSource<'a>,
    error::Error: From<X::Error>,
{
    let read_write_txn = transaction_source.create_read_write_txn()?;
    let read_txn = transaction_source.create_read_txn()?;
    read_write_txn.commit()?;
    read_txn.commit()?;
    Ok(())
}

#[test]
fn in_memory_read_write_transaction_does_not_block_read_transaction() {
    let env = InMemoryEnvironment::new();

    assert!(read_write_transaction_does_not_block_read_transaction::<_>(&env).is_ok())
}

#[test]
fn lmdb_read_write_transaction_does_not_block_read_transaction() {
    let dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();

    assert!(read_write_transaction_does_not_block_read_transaction::<_>(&env).is_ok())
}

fn reads_are_isolated<'a, S, X>(store: &S, env: &'a X) -> Result<(), error::Error>
where
    S: TrieStore<Vec<u8>, Vec<u8>>,
    X: TransactionSource<'a, Handle = S::Handle>,
    error::Error: From<X::Error>,
{
    let TestData(leaf_1_hash, leaf_1) = &super::create_data()[0..1][0];

    {
        let read_txn_1 = env.create_read_txn()?;
        let result = store.get(&read_txn_1, &leaf_1_hash)?;
        assert_eq!(result, None);

        loop {
            let mut write_txn = env.create_read_write_txn()?;
            match store.put(&mut write_txn, leaf_1_hash, leaf_1) {
                Ok(_) => {}
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    write_txn.abort();
                    env.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            }
            write_txn.commit()?;
            break;
        }

        let result = store.get(&read_txn_1, &leaf_1_hash)?;
        read_txn_1.commit()?;
        assert_eq!(result, None);
    }

    {
        let read_txn_2 = env.create_read_txn()?;
        let result = store.get(&read_txn_2, &leaf_1_hash)?;
        read_txn_2.commit()?;
        assert_eq!(result, Some(leaf_1.to_owned()));
    }

    Ok(())
}

#[test]
fn in_memory_reads_are_isolated() {
    let env = InMemoryEnvironment::new();
    let store = InMemoryTrieStore::new(&env, None);

    assert!(reads_are_isolated(&store, &env).is_ok())
}

#[test]
fn lmdb_reads_are_isolated() {
    let dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();
    let store = LmdbTrieStore::new(&env, None, DatabaseFlags::empty()).unwrap();

    assert!(reads_are_isolated(&store, &env).is_ok())
}

fn reads_are_isolated_2<'a, S, X>(store: &S, env: &'a X) -> Result<(), error::Error>
where
    S: TrieStore<Vec<u8>, Vec<u8>>,
    X: TransactionSource<'a, Handle = S::Handle>,
    error::Error: From<X::Error>,
{
    let data = super::create_data();
    let TestData(ref leaf_1_hash, ref leaf_1) = data[0];
    let TestData(ref leaf_2_hash, ref leaf_2) = data[1];

    loop {
        let mut write_txn = env.create_read_write_txn()?;
        match store.put(&mut write_txn, leaf_1_hash, leaf_1) {
            Ok(_) => {}
            Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                write_txn.abort();
                env.grow_map_size()?;
                continue;
            }
            Err(e) => return Err(e),
        }
        write_txn.commit()?;
        break;
    }

    {
        let read_txn_1 = env.create_read_txn()?;

        loop {
            let mut write_txn = env.create_read_write_txn()?;
            match store.put(&mut write_txn, leaf_2_hash, leaf_2) {
                Ok(_) => {}
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    write_txn.abort();
                    env.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            }
            write_txn.commit()?;
            break;
        }

        let result = store.get(&read_txn_1, leaf_1_hash)?;
        read_txn_1.commit()?;
        assert_eq!(result, Some(leaf_1.to_owned()));
    }

    {
        let read_txn_2 = env.create_read_txn()?;
        let result = store.get(&read_txn_2, leaf_2_hash)?;
        read_txn_2.commit()?;
        assert_eq!(result, Some(leaf_2.to_owned()));
    }

    Ok(())
}

#[test]
fn in_memory_reads_are_isolated_2() {
    let env = InMemoryEnvironment::new();
    let store = InMemoryTrieStore::new(&env, None);

    assert!(reads_are_isolated_2(&store, &env).is_ok())
}

#[test]
fn lmdb_reads_are_isolated_2() {
    let dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();
    let store = LmdbTrieStore::new(&env, None, DatabaseFlags::empty()).unwrap();

    assert!(reads_are_isolated_2(&store, &env).is_ok())
}

fn dbs_are_isolated<'a, S, X>(env: &'a X, store_a: &S, store_b: &S) -> Result<(), error::Error>
where
    S: TrieStore<Vec<u8>, Vec<u8>>,
    X: TransactionSource<'a, Handle = S::Handle>,
    X::Error: fmt::Debug,
    error::Error: From<X::Error>,
{
    let data = super::create_data();
    let TestData(ref leaf_1_hash, ref leaf_1) = data[0];
    let TestData(ref leaf_2_hash, ref leaf_2) = data[1];

    loop {
        let mut write_txn = env.create_read_write_txn()?;
        match store_a.put(&mut write_txn, leaf_1_hash, leaf_1) {
            Ok(_) => {}
            Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                write_txn.abort();
                env.grow_map_size()?;
                continue;
            }
            Err(e) => return Err(e),
        }
        match write_txn.commit().map_err(error::Error::from) {
            Ok(_) => break,
            Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                env.grow_map_size()?;
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    loop {
        let mut write_txn = env.create_read_write_txn()?;
        match store_b.put(&mut write_txn, leaf_2_hash, leaf_2) {
            Ok(_) => {}
            Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                env.grow_map_size()?;
                continue;
            }
            Err(e) => return Err(e),
        }
        match write_txn.commit().map_err(error::Error::from) {
            Ok(_) => break,
            Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                env.grow_map_size()?;
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    {
        let read_txn = env.create_read_txn()?;
        let result = store_a.get(&read_txn, leaf_1_hash)?;
        assert_eq!(result, Some(leaf_1.to_owned()));
        let result = store_a.get(&read_txn, leaf_2_hash)?;
        assert_eq!(result, None);
        read_txn.commit()?;
    }

    {
        let read_txn = env.create_read_txn()?;
        let result = store_b.get(&read_txn, leaf_1_hash)?;
        assert_eq!(result, None);
        let result = store_b.get(&read_txn, leaf_2_hash)?;
        assert_eq!(result, Some(leaf_2.to_owned()));
        read_txn.commit()?;
    }

    Ok(())
}

#[test]
fn in_memory_dbs_are_isolated() {
    let env = InMemoryEnvironment::new();
    let store_a = InMemoryTrieStore::new(&env, Some("a"));
    let store_b = InMemoryTrieStore::new(&env, Some("b"));

    assert!(dbs_are_isolated(&env, &store_a, &store_b).is_ok())
}

#[test]
fn lmdb_dbs_are_isolated() {
    let dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();
    let store_a = LmdbTrieStore::new(&env, Some("a"), DatabaseFlags::empty()).unwrap();
    let store_b = LmdbTrieStore::new(&env, Some("b"), DatabaseFlags::empty()).unwrap();

    assert!(dbs_are_isolated(&env, &store_a, &store_b).is_ok())
}

fn transactions_can_be_used_across_sub_databases<'a, S, X>(
    env: &'a X,
    store_a: &S,
    store_b: &S,
) -> Result<(), error::Error>
where
    S: TrieStore<Vec<u8>, Vec<u8>>,
    X: TransactionSource<'a, Handle = S::Handle>,
    X::Error: fmt::Debug,
    error::Error: From<X::Error>,
{
    let data = super::create_data();
    let TestData(ref leaf_1_hash, ref leaf_1) = data[0];
    let TestData(ref leaf_2_hash, ref leaf_2) = data[1];

    {
        loop {
            let mut write_txn = env.create_read_write_txn()?;

            match store_a.put(&mut write_txn, leaf_1_hash, leaf_1) {
                Ok(_) => {}
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    write_txn.abort();
                    env.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            }

            match store_b.put(&mut write_txn, leaf_2_hash, leaf_2) {
                Ok(_) => {}
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    write_txn.abort();
                    env.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            }

            match write_txn.commit().map_err(error::Error::from) {
                Ok(_) => break,
                Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                    env.grow_map_size()?;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
    }

    {
        let read_txn = env.create_read_txn()?;
        let result = store_a.get(&read_txn, leaf_1_hash)?;
        assert_eq!(result, Some(leaf_1.to_owned()));
        let result = store_b.get(&read_txn, leaf_2_hash)?;
        assert_eq!(result, Some(leaf_2.to_owned()));
        read_txn.commit()?;
    }

    Ok(())
}

#[test]
fn in_memory_transactions_can_be_used_across_sub_databases() {
    let env = InMemoryEnvironment::new();
    let store_a = InMemoryTrieStore::new(&env, Some("a"));
    let store_b = InMemoryTrieStore::new(&env, Some("b"));

    assert!(transactions_can_be_used_across_sub_databases(&env, &store_a, &store_b).is_ok());
}

#[test]
fn lmdb_transactions_can_be_used_across_sub_databases() {
    let dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();
    let store_a = LmdbTrieStore::new(&env, Some("a"), DatabaseFlags::empty()).unwrap();
    let store_b = LmdbTrieStore::new(&env, Some("b"), DatabaseFlags::empty()).unwrap();

    assert!(transactions_can_be_used_across_sub_databases(&env, &store_a, &store_b).is_ok());
}

fn uncommitted_transactions_across_sub_databases_do_not_persist<'a, S, X>(
    env: &'a X,
    store_a: &S,
    store_b: &S,
) -> Result<(), error::Error>
where
    S: TrieStore<Vec<u8>, Vec<u8>>,
    X: TransactionSource<'a, Handle = S::Handle>,
    error::Error: From<X::Error>,
{
    let data = super::create_data();
    let TestData(ref leaf_1_hash, ref leaf_1) = data[0];
    let TestData(ref leaf_2_hash, ref leaf_2) = data[1];

    {
        let mut write_txn = env.create_read_write_txn()?;
        store_a.put(&mut write_txn, leaf_1_hash, leaf_1)?;
        store_b.put(&mut write_txn, leaf_2_hash, leaf_2)?;
    }

    {
        let read_txn = env.create_read_txn()?;
        let result = store_a.get(&read_txn, leaf_1_hash)?;
        assert_eq!(result, None);
        let result = store_b.get(&read_txn, leaf_2_hash)?;
        assert_eq!(result, None);
        read_txn.commit()?;
    }

    Ok(())
}

#[test]
fn in_memory_uncommitted_transactions_across_sub_databases_do_not_persist() {
    let env = InMemoryEnvironment::new();
    let store_a = InMemoryTrieStore::new(&env, Some("a"));
    let store_b = InMemoryTrieStore::new(&env, Some("b"));

    assert!(
        uncommitted_transactions_across_sub_databases_do_not_persist(&env, &store_a, &store_b)
            .is_ok()
    );
}

#[test]
fn lmdb_uncommitted_transactions_across_sub_databases_do_not_persist() {
    let dir = tempdir().unwrap();
    let env = LmdbEnvironment::new(&dir.path().to_path_buf(), *TEST_MAP_SIZE).unwrap();
    let store_a = LmdbTrieStore::new(&env, Some("a"), DatabaseFlags::empty()).unwrap();
    let store_b = LmdbTrieStore::new(&env, Some("b"), DatabaseFlags::empty()).unwrap();

    assert!(
        uncommitted_transactions_across_sub_databases_do_not_persist(&env, &store_a, &store_b)
            .is_ok()
    )
}
