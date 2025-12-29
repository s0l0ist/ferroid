use core::fmt;

use crate::{
    generator::{IdGenStatus, Result},
    id::UlidId,
    rand::RandSource,
    time::TimeSource,
};

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
    #[cfg(any(not(feature = "lock"), feature = "parking-lot"))]
    fn next_id(&self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next_id`] that returns a [`Result`].
    ///
    /// # Errors
    /// - May return an error if the underlying generator uses a lock and it is
    ///   poisoned.
    fn try_next_id(&self) -> Result<IdGenStatus<ID>, Self::Err>;
}
