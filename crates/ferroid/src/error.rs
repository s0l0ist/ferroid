use core::fmt;

/// A result type that is infallible by default.
///
/// Most `ferroid` APIs are infallible. However, some fallible variants are
/// enabled behind feature flags like `std` or `base32`.
pub type Result<T, E = core::convert::Infallible> = core::result::Result<T, E>;

/// All possible errors that `ferroid` can produce.
///
/// The generic parameter `E` is only used when the `base32` feature is enabled,
/// in the `Base32Error<E>` variant. To preserve the generic across all feature
/// combinations, other variants carry a `PhantomData<E>`, even if unused.
///
/// When both `std` and `base32` are disabled, `ferroid` is effectively
/// infallible. The `Infallible` variant exists only to satisfy the type system
/// and is never expected to surface in practice.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Error<E = core::convert::Infallible> {
    /// The operation failed due to a poisoned lock.
    ///
    /// This can happen if another thread panicked while holding a shared lock.
    /// Only available when the `std` feature is enabled.
    #[cfg(feature = "std")]
    LockPoisoned(core::marker::PhantomData<E>),

    /// An error occurred during Crockford Base32 decoding.
    ///
    /// This wraps the [`crate::Base32Error`] type and is only available when
    /// the `base32` feature is enabled.
    #[cfg(feature = "base32")]
    Base32Error(crate::Base32Error<E>),

    /// Placeholder variant for `no_std` builds without the `base32` feature.
    ///
    /// When both the `std` and `base32` features are disabled, `ferroid` is
    /// infallible at runtime. This variant exists to satisfy the API's use of
    /// `Result<T, Error<E>>`, but should never be constructed or observed.
    #[cfg(not(all(feature = "std", feature = "base32")))]
    Infallible(core::marker::PhantomData<E>),
}

impl<E: fmt::Debug> fmt::Display for Error<E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{self:?}")
    }
}

impl<E: fmt::Debug> core::error::Error for Error<E> {}

#[cfg(feature = "std")]
use std::sync::{MutexGuard, PoisonError};
#[cfg(feature = "std")]
// Convert all poisoned lock errors to a simplified `LockPoisoned`
impl<T, E: fmt::Debug> From<PoisonError<MutexGuard<'_, T>>> for Error<E> {
    fn from(_: PoisonError<MutexGuard<'_, T>>) -> Self {
        Self::LockPoisoned(core::marker::PhantomData)
    }
}
