use std::path::PathBuf;

use lmdb::{
    self as lmdb_external, Database, DatabaseFlags, Environment, RoTransaction, RwTransaction,
    WriteFlags,
};

use engine_shared::lmdb_ext::EnvironmentExt;

use crate::{
    error::lmdb::Error,
    transaction_source::{Readable, Transaction, TransactionSource, Writable},
    MAX_DBS,
};

const GROWTH_RATIO: usize = 2;

impl<'a> Transaction for RoTransaction<'a> {
    type Error = Error;

    type Handle = Database;

    fn commit(self) -> Result<(), Self::Error> {
        lmdb::Transaction::commit(self).map_err(Error::from)
    }

    fn abort(self) {
        lmdb::Transaction::abort(self)
    }
}

impl<'a> Readable for RoTransaction<'a> {
    fn read(&self, handle: Self::Handle, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        match lmdb::Transaction::get(self, handle, &key) {
            Ok(bytes) => Ok(Some(bytes.to_vec())),
            Err(lmdb::Error::NotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

impl<'a> Transaction for RwTransaction<'a> {
    type Error = Error;

    type Handle = Database;

    fn commit(self) -> Result<(), Self::Error> {
        <RwTransaction<'a> as lmdb::Transaction>::commit(self).map_err(Error::from)
    }

    fn abort(self) {
        <RwTransaction<'a> as lmdb::Transaction>::abort(self)
    }
}

impl<'a> Readable for RwTransaction<'a> {
    fn read(&self, handle: Self::Handle, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        match lmdb::Transaction::get(self, handle, &key) {
            Ok(bytes) => Ok(Some(bytes.to_vec())),
            Err(lmdb::Error::NotFound) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

impl<'a> Writable for RwTransaction<'a> {
    fn write(&mut self, handle: Self::Handle, key: &[u8], value: &[u8]) -> Result<(), Self::Error> {
        self.put(handle, &key, &value, WriteFlags::empty())
            .map_err(Into::into)
    }
}
/// The environment for an LMDB-backed trie store.
///
/// Wraps [`lmdb::Environment`].
#[derive(Debug)]
pub struct LmdbEnvironment {
    path: PathBuf,
    env: Environment,
}

impl LmdbEnvironment {
    pub fn new(path: &PathBuf, map_size: usize) -> Result<Self, Error> {
        let env = Environment::new()
            .set_max_dbs(MAX_DBS)
            .set_map_size(map_size)
            .open(path)
            .map_err(Error::from)?;
        let path = path.to_owned();
        Ok(LmdbEnvironment { path, env })
    }

    pub fn create_db(&self, name: Option<&str>, flags: DatabaseFlags) -> Result<Database, Error> {
        loop {
            match self.env.create_db(name, flags).map_err(Error::from) {
                Ok(db) => return Ok(db),
                Err(e) if e.is_map_full() => {
                    self.grow_map_size()?;
                    continue;
                }
                other => return other,
            }
        }
    }

    pub fn open_db(&self, name: Option<&str>) -> Result<Database, Error> {
        self.env.open_db(name).map_err(Error::from)
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

/// Creates a transaction object using provided function and handles a resized map error
/// transparently.
fn create_retried_txn<T: lmdb::Transaction>(
    env: &Environment,
    begin_fn: impl Fn() -> Result<T, lmdb_external::Error>,
) -> Result<T, Error> {
    loop {
        match begin_fn() {
            Ok(txn) => return Ok(txn),
            Err(lmdb_external::Error::MapResized) => {
                // Map size is increased by another process. Call `mdb_env_set_mapsize` with
                // zero to to adopt to the new size, and then the whole operation is retried.
                env.set_map_size(0)?;
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

impl<'a> TransactionSource<'a> for LmdbEnvironment {
    type Error = Error;

    type Handle = Database;

    type ReadTransaction = RoTransaction<'a>;

    type ReadWriteTransaction = RwTransaction<'a>;

    fn create_read_txn(&'a self) -> Result<Self::ReadTransaction, Self::Error> {
        create_retried_txn(&self.env, || self.env.begin_ro_txn())
    }

    fn create_read_write_txn(&'a self) -> Result<RwTransaction<'a>, Self::Error> {
        create_retried_txn(&self.env, || self.env.begin_rw_txn())
    }

    fn grow_map_size(&self) -> Result<(), Self::Error> {
        let current_map_size = self.env.get_map_size()?;
        self.env.set_map_size(current_map_size * GROWTH_RATIO)?;
        Ok(())
    }
}
