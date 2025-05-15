use crate::{
    AtomicSnowflakeGenerator, BasicSnowflakeGenerator, IdGenStatus, LockSnowflakeGenerator, Result,
    Snowflake, TimeSource,
};

/// A minimal interface for generating Snowflake IDs in a single-threaded
/// context.
///
/// This trait is primarily used for benchmarking and testing different
/// generator implementations. It abstracts over common generator methods
/// without exposing implementation details.
///
/// # Note
/// This trait is not intended for external use and may change or be removed in
/// future versions.
pub trait SnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    // Creates a new generator
    fn new(machine_id: ID::Ty, clock: T) -> Self;

    /// Returns the next available ID or yields if generation is temporarily
    /// stalled.
    fn next(&mut self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next`] that returns a [`Result`].
    fn try_next(&mut self) -> Result<IdGenStatus<ID>>;
}

/// A minimal interface for generating Snowflake IDs in a thread-safe context.
///
/// This trait is primarily used for benchmarking and testing different
/// generator implementations. It abstracts over common generator methods
/// without exposing implementation details.
///
/// # Note
/// This trait is not intended for external use and may change or be removed in
/// future versions.
pub trait MultithreadedSnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    // Creates a new generator
    fn new(machine_id: ID::Ty, clock: T) -> Self;

    /// Returns the next available ID or yields if generation is temporarily
    /// stalled.
    fn next(&self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next`] that returns a [`Result`].
    fn try_next(&self) -> Result<IdGenStatus<ID>>;
}

impl<ID, T> SnowflakeGenerator<ID, T> for BasicSnowflakeGenerator<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::new(machine_id, clock)
    }

    fn next(&mut self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&mut self) -> Result<IdGenStatus<ID>> {
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
    fn next(&mut self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&mut self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}

impl<ID, T> SnowflakeGenerator<ID, T> for AtomicSnowflakeGenerator<ID, T>
where
    ID: Snowflake<Ty = u64>,
    T: TimeSource<u64>,
{
    fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::new(machine_id, clock)
    }

    fn next(&mut self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&mut self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
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

    fn next(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}

impl<T, ID> MultithreadedSnowflakeGenerator<ID, T> for AtomicSnowflakeGenerator<ID, T>
where
    ID: Snowflake<Ty = u64>,
    T: TimeSource<u64>,
{
    fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::new(machine_id, clock)
    }

    fn next(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}
