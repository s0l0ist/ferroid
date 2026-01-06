use core::fmt;

use crate::{
    generator::{Poll, Result},
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
    /// [`SnowflakeGenerator::try_next_id`]. The returned [`Poll`] contains
    /// either:
    /// - the newly generated ID, or
    /// - a duration to yield/sleep if the timestamp sequence is exhausted.
    fn next_id(&self, f: impl FnMut(ID::Ty)) -> ID
    where
        Self::Err: Into<core::convert::Infallible>,
    {
        match self.try_next_id(f) {
            Ok(id) => id,
            Err(e) => {
                #[allow(unreachable_code)]
                // `into()` satisfies the trait bound at compile time.
                match e.into() {}
            }
        }
    }

    /// Generates the next available ID with fallible error handling.
    ///
    /// The returned [`Poll`] contains either:
    /// - the newly generated ID, or
    /// - a duration to yield/sleep if the timestamp sequence is exhausted.
    ///
    /// # Errors
    ///
    /// May return an error if the underlying implementation uses a lock and it
    /// is poisoned.
    fn try_next_id(&self, f: impl FnMut(ID::Ty)) -> Result<ID, Self::Err>;

    /// Attempts to generate the next available ID.
    ///
    /// This is the infallible counterpart to
    /// [`SnowflakeGenerator::try_poll_id`]. The returned [`Poll`] contains
    /// either:
    /// - the newly generated ID, or
    /// - a duration to yield/sleep if the timestamp sequence is exhausted.
    fn poll_id(&self) -> Poll<ID>
    where
        Self::Err: Into<core::convert::Infallible>,
    {
        match self.try_poll_id() {
            Ok(status) => status,
            Err(e) => {
                #[allow(unreachable_code)]
                // `into()` satisfies the trait bound at compile time.
                match e.into() {}
            }
        }
    }

    /// Attempts to generate the next available ID with fallible error handling.
    ///
    /// The returned [`Poll`] contains either:
    /// - the newly generated ID, or
    /// - a duration to yield/sleep if the timestamp sequence is exhausted.
    ///
    /// # Errors
    ///
    /// May return an error if the underlying implementation uses a lock and it
    /// is poisoned.
    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err>;
}
