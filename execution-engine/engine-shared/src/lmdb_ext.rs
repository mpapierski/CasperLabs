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
    use super::EnvironmentExt;
    use lmdb::{self, Environment};

    const DEFAULT_MAP_SIZE: usize = 1024 * 1024;

    #[test]
    fn should_get_and_set_map_size() {
        let tmp_dir = tempfile::tempdir().unwrap();
        println!("tmp_dir: {:?}", tmp_dir.path());

        let env = Environment::new()
            .set_map_size(DEFAULT_MAP_SIZE)
            .open(tmp_dir.path())
            .expect("should open");

        let _db = env.open_db(None).unwrap();

        assert_eq!(
            env.get_map_size().expect("should get map size 1"),
            DEFAULT_MAP_SIZE
        );
        env.set_map_size(DEFAULT_MAP_SIZE * 2)
            .expect("should set map size");
        assert_eq!(
            env.get_map_size().expect("should get map size 2"),
            DEFAULT_MAP_SIZE * 2
        );
        // Opening another handle at this database and verifying the size does not return `map_size
        // * 2` size as expected. This is due to the fact that the database didn't commit
        // new size. Writable transaction part is omitted here as we extensively
        // exercise transaction logic (with properly handled MapResized/MapFull cases) in other
        // parts of the code.
    }
}
