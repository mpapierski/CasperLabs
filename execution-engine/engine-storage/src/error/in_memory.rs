use std::sync;

use failure::Fail;

#[derive(Debug, Clone, Copy, Fail, PartialEq, Eq)]
pub enum Error {
    #[fail(display = "Another thread panicked while holding a lock")]
    Poison,
}

impl<T> From<sync::PoisonError<T>> for Error {
    fn from(_error: sync::PoisonError<T>) -> Self {
        Error::Poison
    }
}
