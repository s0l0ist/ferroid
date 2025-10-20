use core::fmt;

/// A result type that is infallible by default.
///
/// Most `ferroid` APIs are infallible. However, some fallible variants are
/// enabled behind feature flags like `std` or `base32`.
pub type Result<T, E = core::convert::Infallible> = core::result::Result<T, E>;

/// All error variants that `ferroid` can emit.
///
/// The generic parameter `E` is only *material* when the `base32` feature is
/// enabled, where it appears in [`Error::Base32Error`]. In all other cases, the
/// enum carries a `PhantomData<E>` to keep the public type stable across
/// feature combinations.
///
/// When **`base32` is disabled** and either **`lock` is disabled** *or*
/// **`parking-lot` is enabled** (no poisoning), the crate is effectively
/// infallible at runtime. In that configuration, the [`Error::Infallible`]
/// variant exists solely to satisfy the `Result<T, Error<E>>` API and should
/// never be observed in practice.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum Error<E = core::convert::Infallible> {
    /// The operation failed because the lock was **poisoned**.
    ///
    /// This occurs when a thread panics while holding the lock. When the
    /// `parking-lot` feature is enabled, mutexes do **not** poison, so this
    /// variant is not available.
    #[cfg(all(feature = "lock", not(feature = "parking-lot")))]
    LockPoisoned(core::marker::PhantomData<E>),

    /// An error occurred during Crockford Base32 decoding.
    ///
    /// This wraps the [`crate::Base32Error`] type and is only available when
    /// the `base32` feature is enabled.
    #[cfg(feature = "base32")]
    Base32Error(crate::Base32Error<E>),

    /// Placeholder variant for builds where this crate is effectively
    /// **infallible**.
    ///
    /// `ferroid` only produces errors from:
    /// - **Base32** decoding (`base32` feature), or
    /// - **Lock poisoning** when using a std mutex (`lock` **without**
    ///   `parking-lot`).
    ///
    /// If neither of those error sources is enabled (i.e., no `base32`, or
    /// `lock` is disabled, or `parking-lot` is enabled), there is nothing
    /// fallible at runtime. This variant exists solely to satisfy `Result<T,
    /// Error<E>>` and should never be constructed.
    #[cfg(not(all(feature = "lock", feature = "base32")))]
    Infallible(core::marker::PhantomData<E>),
}

impl<E: fmt::Debug> fmt::Display for Error<E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{self:?}")
    }
}

impl<E: fmt::Debug> core::error::Error for Error<E> {}

#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
use crate::{MutexGuard, PoisonError};
#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
// Convert all poisoned lock errors to a simplified `LockPoisoned`
impl<T, E: fmt::Debug> From<PoisonError<MutexGuard<'_, T>>> for Error<E> {
    fn from(_: PoisonError<MutexGuard<'_, T>>) -> Self {
        Self::LockPoisoned(core::marker::PhantomData)
    }
}
