use alloc::sync::Arc;
use core::cmp::Ordering;

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{Error, Mutex, Poll, Result, SnowflakeGenerator},
    id::SnowflakeId,
    time::TimeSource,
};

/// A lock-based Snowflake ID generator suitable for multi-threaded
/// environments.
///
/// This generator wraps the Snowflake state in an [`Arc<Mutex<_>>`], allowing
/// safe shared use across threads.
///
/// ## Features
/// - ✅ Thread-safe
/// - ✅ Safely implement any [`SnowflakeId`] layout
///
/// ## Recommended When
/// - You're in a multi-threaded environment
/// - Fair access across threads is important
/// - Your target doesn't support atomics.
///
/// ## See Also
/// - [`BasicSnowflakeGenerator`]
/// - [`AtomicSnowflakeGenerator`]
///
/// [`BasicSnowflakeGenerator`]: crate::generator::BasicSnowflakeGenerator
/// [`AtomicSnowflakeGenerator`]: crate::generator::AtomicSnowflakeGenerator
pub struct LockSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    #[cfg(feature = "cache-padded")]
    pub(crate) state: Arc<crossbeam_utils::CachePadded<Mutex<ID>>>,
    #[cfg(not(feature = "cache-padded"))]
    pub(crate) state: Arc<Mutex<ID>>,
    pub(crate) time: T,
}

impl<ID, T> LockSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    /// Creates a new [`LockSnowflakeGenerator`] initialized with the current
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
    /// A new [`LockSnowflakeGenerator`] ready to produce unique, time-ordered
    /// IDs.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "parking-lot")] {
    ///     use ferroid::{
    ///         generator::{Poll, LockSnowflakeGenerator},
    ///         id::SnowflakeTwitterId,
    ///         time::{MonotonicClock, TWITTER_EPOCH},
    ///     };
    ///
    ///     let generator = LockSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
    ///
    ///     let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// }
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
            #[cfg(feature = "cache-padded")]
            state: Arc::new(crossbeam_utils::CachePadded::new(Mutex::new(id))),
            #[cfg(not(feature = "cache-padded"))]
            state: Arc::new(Mutex::new(id)),
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
    ///     generator::{LockSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = LockSnowflakeGenerator::new(0, MonotonicClock::default());
    ///
    /// let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// ```
    #[cfg(feature = "parking-lot")]
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
    ///     generator::{LockSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = LockSnowflakeGenerator::new(0, MonotonicClock::default());
    ///
    /// let id: SnowflakeTwitterId = match generator.try_next_id(|_| std::thread::yield_now()) {
    ///     Ok(id) => id,
    ///     Err(_) => unreachable!(),
    /// };
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the generator fails, such as from lock poisoning.
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self, f)))]
    pub fn try_next_id(&self, mut f: impl FnMut(ID::Ty)) -> Result<ID, Error> {
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
    ///     generator::{LockSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{MonotonicClock, TWITTER_EPOCH},
    /// };
    ///
    /// let generator = LockSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
    ///
    /// let id: SnowflakeTwitterId = loop {
    ///     match generator.poll_id() {
    ///         Poll::Ready { id } => break id,
    ///         Poll::Pending { .. } => std::thread::yield_now(),
    ///     }
    /// };
    /// ```
    #[cfg(feature = "parking-lot")]
    pub fn poll_id(&self) -> Poll<ID>
    where
        Error: Into<core::convert::Infallible>,
    {
        match self.try_poll_id() {
            Ok(id) => id,
            Err(e) => {
                #[allow(unreachable_code)]
                // `into()` satisfies the trait bound at compile time.
                match Into::<core::convert::Infallible>::into(e) {}
            }
        }
    }

    /// Attempts to generate a new ULID with fallible error handling.
    ///
    /// This method attempts to generate the next ID based on the current time
    /// and internal state. If successful, it returns [`Poll::Ready`]
    /// with a newly generated ID. If the generator is temporarily exhausted, it
    /// returns [`Poll::Pending`]. If an internal failure occurs (e.g., a
    /// time source or lock error), it returns an error.
    ///
    /// # Returns
    /// - `Ok(Poll::Ready { id })`: A new ID is available
    /// - `Ok(Poll::Pending { yield_for })`: The time to wait (in
    ///   milliseconds) before trying again
    /// - `Err(e)`: the lock was poisoned
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{LockSnowflakeGenerator, Poll},
    ///     id::{SnowflakeTwitterId, ToU64},
    ///     time::{MonotonicClock, TWITTER_EPOCH},
    /// };
    ///
    /// let generator = LockSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
    ///
    /// // Attempt to generate a new ID
    /// let id: SnowflakeTwitterId = loop {
    ///     match generator.try_poll_id() {
    ///         Ok(Poll::Ready { id }) => break id,
    ///         Ok(Poll::Pending { yield_for }) => {
    ///             std::thread::sleep(core::time::Duration::from_millis(yield_for.to_u64()));
    ///         }
    ///         Err(e) => panic!("Generator error: {}", e),
    ///     }
    /// };
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the generator fails, such as from lock poisoning.
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_poll_id(&self) -> Result<Poll<ID>, Error> {
        let now = self.time.current_millis();

        let mut id = {
            #[cfg(feature = "parking-lot")]
            {
                self.state.lock()
            }
            #[cfg(not(feature = "parking-lot"))]
            {
                self.state.lock()?
            }
        };

        let current_ts = id.timestamp();
        match now.cmp(&current_ts) {
            Ordering::Equal => {
                if id.has_sequence_room() {
                    *id = id.increment_sequence();
                    Ok(Poll::Ready { id: *id })
                } else {
                    Ok(Poll::Pending { yield_for: ID::ONE })
                }
            }
            Ordering::Greater => {
                *id = id.rollover_to_timestamp(now);
                Ok(Poll::Ready { id: *id })
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

impl<ID, T> SnowflakeGenerator<ID, T> for LockSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err = Error;

    fn new(machine_id: ID::Ty, time: T) -> Self {
        Self::new(machine_id, time)
    }

    fn try_next_id(&self, f: impl FnMut(ID::Ty)) -> Result<ID, Self::Err> {
        self.try_next_id(f)
    }

    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err> {
        self.try_poll_id()
    }
}
