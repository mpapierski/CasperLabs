use std::collections::BTreeMap;

use types::bytesrepr::{FromBytes, ToBytes};

use crate::{
    error,
    store::{Store, StoreExt},
    transaction_source::{Transaction, TransactionSource},
};

// should be moved to the `store` module
fn roundtrip<'a, K, V, X, S>(
    transaction_source: &'a X,
    store: &S,
    items: &BTreeMap<K, V>,
) -> Result<Vec<Option<V>>, error::Error>
where
    K: ToBytes,
    V: ToBytes + FromBytes,
    X: TransactionSource<'a, Handle = S::Handle>,
    S: Store<K, V>,
    error::Error: From<X::Error>,
{
    loop {
        let mut txn = transaction_source.create_read_write_txn()?;
        match store.put_many(&mut txn, items.iter()) {
            Ok(_) => {}
            Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                txn.abort();
                transaction_source.grow_map_size()?;
                continue;
            }
            Err(e) => return Err(e),
        }
        let result = store.get_many(&txn, items.keys())?;

        match txn.commit().map_err(error::Error::from) {
            Ok(_) => return Ok(result),
            Err(error::Error::Lmdb(e)) if e.is_map_full() => {
                transaction_source.grow_map_size()?;
                continue;
            }
            Err(e) => return Err(e),
        }
    }
}

// should be moved to the `store` module
pub fn roundtrip_succeeds<'a, K, V, X, S>(
    transaction_source: &'a X,
    store: &S,
    items: BTreeMap<K, V>,
) -> Result<bool, error::Error>
where
    K: ToBytes,
    V: ToBytes + FromBytes + Clone + PartialEq,
    X: TransactionSource<'a, Handle = S::Handle>,
    error::Error: From<X::Error>,
    S: Store<K, V>,
{
    let maybe_values: Vec<Option<V>> = roundtrip(transaction_source, store, &items)?;
    let values = match maybe_values.into_iter().collect::<Option<Vec<V>>>() {
        Some(values) => values,
        None => return Ok(false),
    };
    Ok(Iterator::eq(items.values(), values.iter()))
}
