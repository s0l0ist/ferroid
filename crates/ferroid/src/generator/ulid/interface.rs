use crate::{IdGenStatus, RandSource, Result, TimeSource, UlidId};
use core::fmt;

/// A minimal interface for generating Ulid IDs
pub trait UlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    type Err: fmt::Debug;

    // Creates a new generator
    fn new(time: T, rng: R) -> Self;

    /// Returns the next available ID
    fn next_id(&self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next_id`] that returns a [`Result`].
    ///
    /// # Errors
    /// - May return an error if the underlying generator uses a lock and it is
    ///   poisoned.
    fn try_next_id(&self) -> Result<IdGenStatus<ID>, Self::Err>;
}
