use core::fmt;
use derive_more::From;
use std::sync::{MutexGuard, PoisonError};

pub type Result<T> = core::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    LockPoisoned,

    // TODO feature flag
    #[cfg(feature = "base32")]
    DecodeNonAsciiValue,
    #[cfg(feature = "base32")]
    DecodeInvalidLen,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{self:?}")
    }
}

impl core::error::Error for Error {}

// Convert all poisoned lock errors to a simplified `LockPoisoned`
impl<T> From<PoisonError<MutexGuard<'_, T>>> for Error {
    fn from(_: PoisonError<MutexGuard<'_, T>>) -> Self {
        Error::LockPoisoned
    }
}
