use std::mem;

use lmdb::{Environment, Error};
use lmdb_sys::{self, MDB_env, MDB_SUCCESS};

pub struct EnvInfo(lmdb_sys::MDB_envinfo);

impl EnvInfo {
    pub fn get_map_size(&self) -> usize {
        self.0.me_mapsize
    }
}

pub trait EnvironmentExt {
    fn get_env_info(&self) -> Result<EnvInfo, lmdb::Error>;

    fn get_map_size(&self) -> Result<usize, lmdb::Error> {
        let env_info = self.get_env_info()?;
        Ok(env_info.get_map_size())
    }

    fn set_map_size(&self, map_size: usize) -> Result<(), lmdb::Error>;
}

fn lmdb_result(err_code: i32) -> Result<(), Error> {
    if err_code == MDB_SUCCESS {
        Ok(())
    } else {
        Err(Error::from_err_code(err_code))
    }
}

impl EnvironmentExt for Environment {
    fn get_env_info(&self) -> Result<EnvInfo, lmdb::Error> {
        let env: *mut MDB_env = self.env();
        let e = mem::MaybeUninit::uninit();
        unsafe {
            let mut env_info = EnvInfo(e.assume_init());
            lmdb_result(lmdb_sys::mdb_env_info(env, &mut env_info.0))?;
            Ok(env_info)
        }
    }

    fn set_map_size(&self, map_size: usize) -> Result<(), Error> {
        let env: *mut MDB_env = self.env();
        unsafe { lmdb_result(lmdb_sys::mdb_env_set_mapsize(env, map_size)) }
    }
}

#[cfg(test)]
mod tests {
    use lmdb::{Environment, Transaction, WriteFlags};
    use tempfile;

    use super::*;
    use crate::os;

    const DEFAULT_MAP_SIZE: usize = 10485760;
    const GARBAGE: usize = 16392;

    #[test]
    fn should_resize_map() {
        let mut map_size = 68719476736;

        let tmp_dir = tempfile::tempdir().unwrap();

        println!("tmp_dir: {:?}", tmp_dir.path());

        {
            let env = Environment::new()
                .set_map_size(map_size)
                .open(tmp_dir.path())
                .unwrap();

            let db = env.open_db(None).unwrap();

            let test_val = vec![1u8; 2147467256];

            let mut i: u8 = 1;

            loop {
                println!("i {:?}", i);
                let test_key = vec![i; TEST_KEY_LENGTH];

                let mut txn = env.begin_rw_txn().unwrap();
                match txn.put(db, &test_key, &test_val, WriteFlags::empty()) {
                    Ok(_) => {
                        txn.commit().unwrap();
                    }
                    Err(lmdb::Error::MapFull) => {
                        txn.abort();
                        map_size = map_size * 2;
                        env.set_map_size(map_size).unwrap();
                        println!("resized: {:?}", map_size);
                        i += 1;
                        break;
                    }
                    e => {
                        txn.abort();
                        e.unwrap()
                    }
                }
                i += 1;
            }

            println!("i {:?}", i);
            let test_key = vec![i; TEST_KEY_LENGTH];

            let mut txn = env.begin_rw_txn().unwrap();
            txn.put(db, &test_key, &test_val, WriteFlags::empty())
                .unwrap();
            txn.commit().unwrap();

            let map_size_actual = env.get_map_size().unwrap();
            println!("map_size_actual: {}", map_size_actual);
            assert_eq!(map_size_actual, map_size);
        }

        {
            let env = Environment::new().open(tmp_dir.path()).unwrap();

            let _db = env.open_db(None).unwrap();

            let map_size_actual = env.get_map_size().unwrap();
            println!("map_size_actual: {}", map_size_actual);
            assert_eq!(map_size_actual, map_size);
        }
    }
}
