mod store_ext;
#[cfg(test)]
pub(crate) mod tests;

use types::bytesrepr::{self, FromBytes, ToBytes};

pub use self::store_ext::StoreExt;

use crate::{
    error,
    transaction_source::{Readable, Writable},
};

pub trait Store<K, V> {
    type Handle;

    fn handle(&self) -> Self::Handle;

    fn get<T>(&self, txn: &T, key: &K) -> Result<Option<V>, error::Error>
    where
        T: Readable<Handle = Self::Handle>,
        T::Error: Into<error::Error>,
        error::Error: From<T::Error>,
        K: ToBytes,
        V: FromBytes,
    {
        let handle = self.handle();
        match txn.read(handle, &key.to_bytes()?)? {
            None => Ok(None),
            Some(value_bytes) => {
                let value = bytesrepr::deserialize(value_bytes)?;
                Ok(Some(value))
            }
        }
    }

    fn put<T>(&self, txn: &mut T, key: &K, value: &V) -> Result<(), error::Error>
    where
        T: Writable<Handle = Self::Handle>,
        T::Error: Into<error::Error>,
        error::Error: From<T::Error>,
        K: ToBytes,
        V: ToBytes,
    {
        let handle = self.handle();
        Ok(txn.write(handle, &key.to_bytes()?, &value.to_bytes()?)?)
    }
}
