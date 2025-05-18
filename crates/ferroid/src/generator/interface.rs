use crate::{
    AtomicSnowflakeGenerator, BasicSnowflakeGenerator, IdGenStatus, LockSnowflakeGenerator, Result,
    Snowflake, TimeSource,
};

/// A minimal interface for generating Snowflake IDs in a single-threaded
/// context.
pub trait SnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    // Creates a new generator
    fn new(machine_id: ID::Ty, clock: T) -> Self;

    /// Returns the next available ID or yields if generation is temporarily
    /// stalled.
    fn next_id(&mut self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next`] that returns a [`Result`].
    fn try_next_id(&mut self) -> Result<IdGenStatus<ID>>;
}

/// A minimal interface for generating Snowflake IDs in a thread-safe context.
pub trait MultithreadedSnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    // Creates a new generator
    fn new(machine_id: ID::Ty, clock: T) -> Self;

    /// Returns the next available ID or yields if generation is temporarily
    /// stalled.
    fn next_id(&self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next`] that returns a [`Result`].
    fn try_next_id(&self) -> Result<IdGenStatus<ID>>;
}

impl<ID, T> SnowflakeGenerator<ID, T> for BasicSnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::new(machine_id, clock)
    }

    fn next_id(&mut self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next_id(&mut self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}

impl<T, ID> SnowflakeGenerator<ID, T> for LockSnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::new(machine_id, clock)
    }
    fn next_id(&mut self) -> IdGenStatus<ID> {
        (&*self).next_id()
    }

    fn try_next_id(&mut self) -> Result<IdGenStatus<ID>> {
        (&*self).try_next_id()
    }
}

impl<ID, T> SnowflakeGenerator<ID, T> for AtomicSnowflakeGenerator<ID, T>
where
    ID: Snowflake<Ty = u64>,
    T: TimeSource<ID::Ty>,
{
    fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::new(machine_id, clock)
    }

    fn next_id(&mut self) -> IdGenStatus<ID> {
        (&*self).next_id()
    }

    fn try_next_id(&mut self) -> Result<IdGenStatus<ID>> {
        (&*self).try_next_id()
    }
}

impl<T, ID> MultithreadedSnowflakeGenerator<ID, T> for LockSnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::new(machine_id, clock)
    }

    fn next_id(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next_id(&self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}

impl<T, ID> MultithreadedSnowflakeGenerator<ID, T> for AtomicSnowflakeGenerator<ID, T>
where
    ID: Snowflake<Ty = u64>,
    T: TimeSource<ID::Ty>,
{
    fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::new(machine_id, clock)
    }

    fn next_id(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next_id(&self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}
