pub mod in_memory;
pub mod lmdb;

use failure::Fail;

use types::bytesrepr;

#[derive(Debug, Clone, Fail, PartialEq, Eq)]
pub enum Error {
    #[fail(display = "{}", _0)]
    Lmdb(#[fail(cause)] lmdb::Error),

    #[fail(display = "{}", _0)]
    InMemory(#[fail(cause)] in_memory::Error),

    #[fail(display = "{}", _0)]
    BytesRepr(#[fail(cause)] bytesrepr::Error),
}

impl wasmi::HostError for Error {}

impl From<lmdb::Error> for Error {
    fn from(error: lmdb::Error) -> Error {
        Error::Lmdb(error)
    }
}

impl From<in_memory::Error> for Error {
    fn from(error: in_memory::Error) -> Error {
        Error::InMemory(error)
    }
}

impl From<bytesrepr::Error> for Error {
    fn from(error: bytesrepr::Error) -> Error {
        Error::BytesRepr(error)
    }
}
