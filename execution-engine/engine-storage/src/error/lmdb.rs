use failure::Fail;
use lmdb as lmdb_external;

#[derive(Debug, Clone, Fail, PartialEq, Eq)]
pub enum Error {
    #[fail(display = "{}", _0)]
    Lmdb(#[fail(cause)] lmdb_external::Error),
}

impl Error {
    pub fn is_map_full(&self) -> bool {
        match self {
            Error::Lmdb(lmdb_external::Error::MapFull) => true,
            _ => false,
        }
    }
}

impl From<lmdb_external::Error> for Error {
    fn from(error: lmdb_external::Error) -> Self {
        Error::Lmdb(error)
    }
}
