use core::fmt;

pub type Result<T, E = core::convert::Infallible> = core::result::Result<T, Error<E>>;

#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Error<E = core::convert::Infallible> {
    #[cfg(feature = "std")]
    LockPoisoned(core::marker::PhantomData<E>),
    #[cfg(feature = "base32")]
    Base32Error(crate::Base32Error<E>),
    // If no_std and no base32, use a dummy `Infallible` variant. This keeps the
    // API the same, but the user should never see this error surface in
    // practice.
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
        Error::LockPoisoned(core::marker::PhantomData)
    }
}
