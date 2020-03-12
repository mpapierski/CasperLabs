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

fn lmdb_result_from(err_code: i32) -> Result<(), Error> {
    if err_code == MDB_SUCCESS {
        Ok(())
    } else {
        Err(Error::from_err_code(err_code))
    }
}

impl EnvironmentExt for Environment {
    fn get_env_info(&self) -> Result<EnvInfo, lmdb::Error> {
        let env_info = {
            let mut env_info = mem::MaybeUninit::uninit();
            let env: *mut MDB_env = self.env();
            let ret = unsafe { lmdb_sys::mdb_env_info(env, env_info.as_mut_ptr()) };
            lmdb_result_from(ret)?;
            unsafe { env_info.assume_init() }
        };
        Ok(EnvInfo(env_info))
    }

    fn set_map_size(&self, map_size: usize) -> Result<(), Error> {
        let env: *mut MDB_env = self.env();
        let ret = unsafe { lmdb_sys::mdb_env_set_mapsize(env, map_size) };
        lmdb_result_from(ret)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmdb::{self, Environment, Transaction, WriteFlags};
    use std::{path::Path, thread};

    const DEFAULT_MAP_SIZE: usize = 1024 * 1024;
    const TEST_KEY_LENGTH: usize = 32;

    fn test_thread(thread_no: usize, path: &Path) {
        let mut map_size = DEFAULT_MAP_SIZE;
        let env = Environment::new()
            .set_map_size(map_size)
            .open(path)
            .unwrap();

        let db = env.open_db(None).unwrap();

        for val in 0..255 {
            let test_val = vec![val as u8; 1024 * 1024];
            let mut test_key = vec![val; TEST_KEY_LENGTH];
            debug_assert!(thread_no <= 255);
            test_key[0] = thread_no as u8;

            for i in 1.. {
                let mut txn = None;
                for _retry in 0.. {
                    txn = match env.begin_rw_txn() {
                        Ok(txn) => Some(txn),
                        Err(lmdb::Error::MapResized) => {
                            env.set_map_size(0).expect("should set map size to 0");
                            continue;
                        }
                        Err(e) => panic!("should begin rw txn val={} i={}: {:?}", val, i, e),
                    };
                    break;
                }

                let mut txn = txn.unwrap();
                match txn.put(db, &test_key, &test_val, WriteFlags::empty()) {
                    Ok(_) => txn.commit().unwrap(),
                    Err(lmdb::Error::MapFull) => {
                        txn.abort();
                        map_size *= 2;
                        env.set_map_size(map_size).unwrap();
                        continue;
                    }
                    e => {
                        txn.abort();
                        e.unwrap()
                    }
                }
            }
        }
    }

    use std::sync::Arc;
    #[test]
    fn should_resize_map() {
        let mut _map_size = DEFAULT_MAP_SIZE;

        let tmp_dir = tempfile::tempdir().unwrap();
        println!("tmp_dir: {:?}", tmp_dir.path());
        let path = Arc::new(tmp_dir);

        {
            let mut threads = Vec::new();

            // let path = tmp_dir.clone().path();
            for thread_no in 0..10 {
                let p = path.clone();
                let handle = thread::spawn(move || {
                    test_thread(thread_no, p.path());
                });
                threads.push(handle);
            }

            for l in threads {
                println!("joining thread");
                l.join().expect("join failed");
            }
            println!("done");
        }

        {
            let env = Environment::new().open(path.path()).unwrap();

            let _db = env.open_db(None).unwrap();

            let map_size_actual = env.get_map_size().unwrap();
            println!("map_size_actual: {}", map_size_actual);
        }
    }
}
