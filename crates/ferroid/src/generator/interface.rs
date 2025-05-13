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
pub trait SnowflakeGenerator<ID: Snowflake> {
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
pub trait MultithreadedSnowflakeGenerator<ID: Snowflake> {
    /// Returns the next available ID or yields if generation is temporarily
    /// stalled.
    fn next(&self) -> IdGenStatus<ID>;

    /// A fallible version of [`Self::next`] that returns a [`Result`].
    fn try_next(&self) -> Result<IdGenStatus<ID>>;
}

impl<T, ID> SnowflakeGenerator<ID> for BasicSnowflakeGenerator<T, ID>
where
    T: TimeSource<ID::Ty>,
    ID: Snowflake,
{
    fn next(&mut self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&mut self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}

impl<T, ID> SnowflakeGenerator<ID> for LockSnowflakeGenerator<T, ID>
where
    T: TimeSource<ID::Ty>,
    ID: Snowflake,
{
    fn next(&mut self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&mut self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}

impl<T, ID> SnowflakeGenerator<ID> for AtomicSnowflakeGenerator<T, ID>
where
    T: TimeSource<u64>,
    ID: Snowflake<Ty = u64>,
{
    fn next(&mut self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&mut self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}

impl<T, ID> MultithreadedSnowflakeGenerator<ID> for LockSnowflakeGenerator<T, ID>
where
    T: TimeSource<ID::Ty>,
    ID: Snowflake,
{
    fn next(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}

impl<T, ID> MultithreadedSnowflakeGenerator<ID> for AtomicSnowflakeGenerator<T, ID>
where
    T: TimeSource<u64>,
    ID: Snowflake<Ty = u64>,
{
    fn next(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next(&self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}
