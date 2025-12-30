use core::fmt;

use crate::{
    generator::{IdGenStatus, Result},
    id::SnowflakeId,
    time::TimeSource,
};

/// A minimal interface for generating Snowflake IDs.
pub trait SnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    /// The error type returned by [`SnowflakeGenerator::try_next_id`].
    type Err: fmt::Debug;

    /// Creates a new generator.
    fn new(machine_id: ID::Ty, time: T) -> Self;

    /// Generates the next available ID.
    ///
    /// This is the infallible counterpart to
    /// [`SnowflakeGenerator::try_next_id`]. The returned [`IdGenStatus`]
    /// contains either:
    /// - the newly generated ID, or
    /// - a duration to yield/sleep if the timestamp sequence is exhausted.
    #[cfg(any(not(feature = "lock"), feature = "parking-lot"))]
    fn next_id(&self) -> IdGenStatus<ID>;

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
