use core::{cell::Cell, cmp::Ordering};

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{Poll, Result, SnowflakeGenerator},
    id::SnowflakeId,
    time::TimeSource,
};

/// A non-concurrent Snowflake ID generator suitable for single-threaded
/// environments.
///
/// This generator is lightweight and fast, but **not thread-safe**.
///
/// ## Features
/// - ❌ Not thread-safe
/// - ✅ Safely implement any [`SnowflakeId`] layout
///
/// ## Recommended When
/// - You're in a single-threaded environment (no shared access)
/// - You want the fastest generator
///
/// ## See Also
/// - [`LockSnowflakeGenerator`]
/// - [`AtomicSnowflakeGenerator`]
///
/// [`LockSnowflakeGenerator`]: crate::generator::LockSnowflakeGenerator
/// [`AtomicSnowflakeGenerator`]: crate::generator::AtomicSnowflakeGenerator
pub struct BasicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    state: Cell<ID>,
    time: T,
}

impl<ID, T> BasicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    /// Creates a new [`BasicSnowflakeGenerator`] initialized with the current
    /// time and a given machine ID.
    ///
    /// This constructor sets the initial timestamp and sequence to zero, and
    /// uses the provided `time` to fetch the current time during ID generation.
    /// It is the recommended way to create a new atomic generator for typical
    /// use cases.
    ///
    /// # Parameters
    ///
    /// - `machine_id`: A unique identifier for the node or instance generating
    ///   IDs. This value will be encoded into every generated ID.
    /// - `time`: A [`TimeSource`] implementation (e.g., [`MonotonicClock`])
    ///   that determines how timestamps are generated.
    ///
    /// # Returns
    ///
    /// A new [`BasicSnowflakeGenerator`] ready to produce unique, time-ordered
    /// IDs.
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{BasicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{MonotonicClock, TWITTER_EPOCH},
    /// };
    ///
    /// let generator = BasicSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
    ///
    /// let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// ```
    ///
    /// [`TimeSource`]: crate::time::TimeSource
    /// [`MonotonicClock`]: crate::time::MonotonicClock
    pub fn new(machine_id: ID::Ty, time: T) -> Self {
        Self::from_components(ID::ZERO, machine_id, ID::ZERO, time)
    }

    /// Creates a new ID generator from explicit component values.
    ///
    /// This constructor is primarily useful for advanced use cases such as
    /// restoring state from persistent storage or controlling the starting
    /// point of the generator manually.
    ///
    /// # Parameters
    /// - `timestamp`: The initial timestamp component (usually in milliseconds)
    /// - `machine_id`: The machine or worker identifier
    /// - `sequence`: The initial sequence number
    /// - `time`: A [`TimeSource`] implementation used to fetch the current time
    ///
    /// # Returns
    /// A new generator instance preloaded with the given state.
    ///
    /// # ⚠️ Note
    /// In typical use cases, you should prefer [`Self::new`] to let the
    /// generator initialize itself from the current time.
    pub fn from_components(
        timestamp: ID::Ty,
        machine_id: ID::Ty,
        sequence: ID::Ty,
        time: T,
    ) -> Self {
        let id = ID::from_components(timestamp, machine_id, sequence);
        Self {
            state: Cell::new(id),
            time,
        }
    }

    /// Generates a new ID.
    ///
    /// Returns a new, time-ordered, unique ID.
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{BasicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicSnowflakeGenerator::new(0, MonotonicClock::default());
    ///
    /// let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// ```
    pub fn next_id(&self, f: impl FnMut(ID::Ty)) -> ID {
        match self.try_next_id(f) {
            Ok(id) => id,
            Err(e) =>
            {
                #[allow(unreachable_code)]
                match e {}
            }
        }
    }

    /// Generates a new ID.
    ///
    /// Returns a new, time-ordered, unique ID with fallible error handling.
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{BasicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicSnowflakeGenerator::new(0, MonotonicClock::default());
    ///
    /// let id: SnowflakeTwitterId = match generator.try_next_id(|_| std::thread::yield_now()) {
    ///     Ok(id) => id,
    ///     Err(_) => unreachable!(),
    /// };
    /// ```
    ///
    /// # Errors
    ///
    /// This method is infallible for this generator. Use the [`Self::next_id`]
    /// method instead.
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self, f)))]
    pub fn try_next_id(&self, mut f: impl FnMut(ID::Ty)) -> Result<ID> {
        loop {
            match self.try_poll_id()? {
                Poll::Ready { id } => break Ok(id),
                Poll::Pending { yield_for } => f(yield_for),
            }
        }
    }

    /// Attempts to generate the next available ID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the sequence is full and the
    /// time has not advanced), it returns [`Poll::Pending`].
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{BasicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{MonotonicClock, TWITTER_EPOCH},
    /// };
    ///
    /// let generator = BasicSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
    ///
    /// let id: SnowflakeTwitterId = loop {
    ///     match generator.poll_id() {
    ///         Poll::Ready { id } => break id,
    ///         Poll::Pending { .. } => std::thread::yield_now(),
    ///     }
    /// };
    /// ```
    pub fn poll_id(&self) -> Poll<ID> {
        match self.try_poll_id() {
            Ok(id) => id,
            Err(e) =>
            {
                #[allow(unreachable_code)]
                match e {}
            }
        }
    }

    /// A fallible version of [`Self::poll_id`] that returns a [`Result`].
    ///
    /// This method attempts to generate the next ID based on the current time
    /// and internal state. If successful, it returns [`Poll::Ready`] with a
    /// newly generated ID. If the generator is temporarily exhausted, it
    /// returns [`Poll::Pending`]. If an internal failure occurs (e.g., a time
    /// source or lock error), it returns an error.
    ///
    /// # Returns
    /// - `Ok(Poll::Ready { id })`: A new ID is available
    /// - `Ok(Poll::Pending { yield_for })`: The time to wait (in milliseconds)
    ///   before trying again
    /// - `Err(_)`: infallible for this generator
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{BasicSnowflakeGenerator, Poll},
    ///     id::{SnowflakeTwitterId, ToU64},
    ///     time::{MonotonicClock, TWITTER_EPOCH},
    /// };
    ///
    /// let generator = BasicSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
    ///
    /// // Attempt to generate a new ID
    /// let id: SnowflakeTwitterId = loop {
    ///     match generator.try_poll_id() {
    ///         Ok(Poll::Ready { id }) => break id,
    ///         Ok(Poll::Pending { yield_for }) => {
    ///             std::thread::sleep(core::time::Duration::from_millis(yield_for.to_u64()));
    ///         }
    ///         Err(_) => unreachable!(),
    ///     }
    /// };
    /// ```
    ///
    /// # Errors
    ///
    /// This method is infallible for this generator. Use the [`Self::poll_id`]
    /// method instead.
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_poll_id(&self) -> Result<Poll<ID>> {
        let now = self.time.current_millis();
        let state = self.state.get();
        let current_ts = state.timestamp();

        match now.cmp(&current_ts) {
            Ordering::Equal => {
                if state.has_sequence_room() {
                    let updated = state.increment_sequence();
                    self.state.set(updated);
                    Ok(Poll::Ready { id: updated })
                } else {
                    Ok(Poll::Pending { yield_for: ID::ONE })
                }
            }
            Ordering::Greater => {
                let updated = state.rollover_to_timestamp(now);
                self.state.set(updated);
                Ok(Poll::Ready { id: updated })
            }
            Ordering::Less => Ok(Self::cold_clock_behind(now, current_ts)),
        }
    }

    #[cold]
    #[inline(never)]
    fn cold_clock_behind(now: ID::Ty, current_ts: ID::Ty) -> Poll<ID> {
        let yield_for = current_ts - now;
        debug_assert!(yield_for >= ID::ZERO);
        Poll::Pending { yield_for }
    }
}

impl<ID, T> SnowflakeGenerator<ID, T> for BasicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err = core::convert::Infallible;

    fn new(machine_id: ID::Ty, time: T) -> Self {
        Self::new(machine_id, time)
    }

    fn next_id(&self, f: impl FnMut(ID::Ty)) -> ID {
        self.next_id(f)
    }

    fn try_next_id(&self, f: impl FnMut(ID::Ty)) -> Result<ID, Self::Err> {
        self.try_next_id(f)
    }

    fn poll_id(&self) -> Poll<ID> {
        self.poll_id()
    }

    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err> {
        self.try_poll_id()
    }
}
