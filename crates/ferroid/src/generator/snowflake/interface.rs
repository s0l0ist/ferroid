use core::{convert::Infallible, fmt, time::Duration};

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
    /// [`SnowflakeGenerator::try_next_id`]. Calls the backoff strategy `f`
    /// whenever the generator is pending, until an ID is available.
    fn next_id(&self, f: impl FnMut(Duration)) -> ID
    where
        Self::Err: Into<Infallible>,
    {
        let Ok(id) = self.try_next_id(f).map_err(Into::<Infallible>::into);
        id
    }

    /// Generates the next available ID with fallible error handling.
    ///
    /// The default implementation polls until an ID is available or an error
    /// occurs. On each [`Poll::Pending`], it calls the backoff strategy `f`
    /// with the suggested [`Duration`], including [`Duration::ZERO`], before
    /// polling again. Clock granularity is already applied; waits saturate at
    /// `u64::MAX` milliseconds. Ready results and errors do not invoke `f`.
    ///
    /// # Errors
    ///
    /// May return an error if the underlying implementation uses a lock and it
    /// is poisoned.
    #[cfg_attr(
        feature = "tracing",
        tracing::instrument(level = "trace", skip(self, f))
    )]
    fn try_next_id(&self, mut f: impl FnMut(Duration)) -> Result<ID, Self::Err> {
        loop {
            match self.try_poll_id()? {
                Poll::Ready { id } => return Ok(id),
                Poll::Pending { yield_for } => f(yield_for),
            }
        }
    }

    /// Attempts to generate the next available ID.
    ///
    /// This is the infallible counterpart to
    /// [`SnowflakeGenerator::try_poll_id`]. The returned [`Poll`] contains
    /// either:
    /// - the newly generated ID, or
    /// - a suggested [`Duration`] before retrying, including
    ///   [`Duration::ZERO`] when contention permits an immediate retry.
    fn poll_id(&self) -> Poll<ID>
    where
        Self::Err: Into<Infallible>,
    {
        let Ok(status) = self.try_poll_id().map_err(Into::<Infallible>::into);
        status
    }

    /// Attempts to generate the next available ID with fallible error handling.
    ///
    /// The returned [`Poll`] contains either:
    /// - the newly generated ID, or
    /// - a suggested [`Duration`] before retrying, including
    ///   [`Duration::ZERO`] when contention permits an immediate retry.
    ///
    /// # Errors
    ///
    /// May return an error if the underlying implementation uses a lock and it
    /// is poisoned.
    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err>;
}
