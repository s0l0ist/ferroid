use core::fmt;

/// A result type that is infallible by default.
///
/// Most `ferroid` APIs are infallible. However, some fallible variants are
/// enabled behind feature flags like `lock`.
pub type Result<T, E = core::convert::Infallible> = core::result::Result<T, E>;

/// Errors returned by fallible ID generators.
///
/// The variants are independent of enabled features. Generators that cannot
/// fail use [`core::convert::Infallible`] as their associated error type.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Error {
    /// The operation failed because the lock was **poisoned**.
    ///
    /// This occurs when a thread panics while holding the lock. When the
    /// `parking-lot` feature is enabled, mutexes do **not** poison and cannot
    /// produce this error.
    LockPoisoned,
}

impl fmt::Display for Error {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{self:?}")
    }
}

impl core::error::Error for Error {}

#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_: std::sync::PoisonError<T>) -> Self {
        Self::LockPoisoned
    }
}
