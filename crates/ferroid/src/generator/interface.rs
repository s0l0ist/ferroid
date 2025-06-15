use crate::{Fluid, IdGenStatus, Result, Snowflake, TimeSource, rand::RandSource};

/// A minimal interface for generating Snowflake IDs
pub trait SnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    // Creates a new generator
    fn new(machine_id: ID::Ty, clock: T) -> Self;

    /// Returns the next available ID
    fn next_id(&self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next_id`] that returns a [`Result`].
    fn try_next_id(&self) -> Result<IdGenStatus<ID>>;
}

/// A minimal interface for generating FLuid IDs
pub trait FluidGenerator<ID, T, R>
where
    ID: Fluid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    // Creates a new generator
    fn new(clock: T, rng: R) -> Self;

    /// Returns the next available ID
    fn next_id(&mut self) -> ID;
    
    /// A fallible version of [`Self::next_id`] that returns a [`Result`].
    fn try_next_id(&mut self) -> Result<ID>;
}
