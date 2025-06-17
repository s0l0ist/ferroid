use crate::{IdGenStatus, RandSource, Result, TimeSource, Ulid};

/// A minimal interface for generating Ulid IDs
pub trait UlidGenerator<ID, T, R>
where
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    // Creates a new generator
    fn new(clock: T, rng: R) -> Self;

    /// Returns the next available ID
    fn next_id(&self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next_id`] that returns a [`Result`].
    fn try_next_id(&self) -> Result<IdGenStatus<ID>>;
}
