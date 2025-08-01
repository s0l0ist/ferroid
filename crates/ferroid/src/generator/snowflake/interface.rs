use crate::{IdGenStatus, Result, Snowflake, TimeSource};
use core::fmt;

/// A minimal interface for generating Snowflake IDs
pub trait SnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    type Err: fmt::Debug;

    // Creates a new generator
    fn new(machine_id: ID::Ty, clock: T) -> Self;

    /// Returns the next available ID
    fn next_id(&self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next_id`] that returns a [`Result`].
    ///
    /// # Errors
    /// - May return an error if the underlying generator uses a lock and it is
    ///   poisoned.
    fn try_next_id(&self) -> Result<IdGenStatus<ID>, Self::Err>;
}
