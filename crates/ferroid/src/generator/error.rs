use core::fmt;

/// A result type that is infallible by default.
///
/// Most `ferroid` APIs are infallible. However, some fallible variants are
/// enabled behind feature flags like `lock`.
pub type Result<T, E = core::convert::Infallible> = core::result::Result<T, E>;

/// All error variants that `ferroid` can emit.
///
/// When either **`lock` is disabled** *or* **`parking-lot` is enabled** (no
/// poisoning), the crate is effectively infallible at runtime. In that
/// configuration, the `Error::Infallible` variant exists solely to satisfy the
/// `Result<T, Error>` API and should never be observed in practice.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Error {
    /// The operation failed because the lock was **poisoned**.
    ///
    /// This occurs when a thread panics while holding the lock. When the
    /// `parking-lot` feature is enabled, mutexes do **not** poison, so this
    /// variant is not available.
    #[cfg(all(feature = "lock", not(feature = "parking-lot")))]
    LockPoisoned,

    /// Placeholder variant for builds where this crate is effectively
    /// **infallible**.
    ///
    /// `ferroid` only produces errors from lock poisoning when using a std
    /// mutex (`lock` **without** `parking-lot`).
    ///
    /// If lock poisoning cannot occur (`lock` is disabled, or `parking-lot` is
    /// enabled), there is nothing fallible at runtime. This variant exists
    /// solely to satisfy `Result<T, Error>` and cannot be constructed.
    #[cfg(any(not(feature = "lock"), feature = "parking-lot"))]
    Infallible(core::convert::Infallible),
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{self:?}")
    }
}

impl core::error::Error for Error {}

#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
use crate::generator::{MutexGuard, PoisonError};
#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
impl<T> From<PoisonError<MutexGuard<'_, T>>> for Error {
    fn from(_: PoisonError<MutexGuard<'_, T>>) -> Self {
        Self::LockPoisoned
    }
}

#[cfg(any(not(feature = "lock"), feature = "parking-lot"))]
impl From<Error> for core::convert::Infallible {
    fn from(e: Error) -> Self {
        match e {
            Error::Infallible(i) => i,
        }
    }
}
