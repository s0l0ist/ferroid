use core::fmt;

use crate::{
    generator::{IdGenStatus, Result},
    id::UlidId,
    rand::RandSource,
    time::TimeSource,
};

/// A minimal interface for generating ULIDs.
pub trait UlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// The error type returned by [`UlidGenerator::try_next_id`].
    type Err: fmt::Debug;

    /// Creates a new generator.
    fn new(time: T, rng: R) -> Self;

    /// Generates the next available ID.
    ///
    /// This is the infallible counterpart to [`UlidGenerator::try_next_id`].
    /// The returned [`IdGenStatus`] contains either:
    /// - the newly generated ID, or
    /// - a duration to yield/sleep if the timestamp sequence is exhausted.
    #[cfg(any(not(feature = "lock"), feature = "parking-lot"))]
    fn next_id(&self) -> IdGenStatus<ID>
    where
        Self::Err: Into<core::convert::Infallible>;

    /// Generates the next available ID.
    ///
    /// The returned [`IdGenStatus`] contains either:
    /// - the newly generated ID, or
    /// - a duration to yield/sleep if the timestamp sequence is exhausted.
    ///
    /// # Errors
    ///
    /// May return an error if the underlying implementation uses a lock and it
    /// is poisoned.
    fn try_next_id(&self) -> Result<IdGenStatus<ID>, Self::Err>;
}
