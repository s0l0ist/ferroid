use core::{cmp, marker::PhantomData, time::Duration};

use portable_atomic::{AtomicU64, Ordering};
#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{Poll, Result, SnowflakeGenerator},
    id::{SnowflakeId, ToU64},
    time::TimeSource,
};

/// A lock-free Snowflake ID generator suitable for multi-threaded environments.
///
/// This generator stores the Snowflake state in an [`AtomicU64`], allowing safe
/// shared use across threads.
///
/// ## Features
/// - ✅ Thread-safe
/// - ❌ Safely implement any [`SnowflakeId`] layout
///
/// ## Caveats
/// This implementation uses an [`AtomicU64`] internally, so it only supports ID
/// layouts where the underlying type is [`u64`]. You cannot use layouts with
/// larger or smaller representations (i.e., `ID::Ty` must be [`u64`]).
///
/// ## Recommended When
/// - You're in a multi-threaded environment
/// - Fair access is sacrificed for higher throughput
///
/// ## See Also
/// - [`BasicSnowflakeGenerator`]
/// - [`LockSnowflakeGenerator`]
///
/// [`BasicSnowflakeGenerator`]: crate::generator::BasicSnowflakeGenerator
/// [`LockSnowflakeGenerator`]: crate::generator::LockSnowflakeGenerator
pub struct AtomicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId<Ty = u64>,
    T: TimeSource<ID::Ty>,
{
    state: crossbeam_utils::CachePadded<AtomicU64>,
    time: T,
    _id: PhantomData<ID>,
}

impl<ID, T> AtomicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId<Ty = u64>,
    T: TimeSource<ID::Ty>,
{
    /// Creates a new [`AtomicSnowflakeGenerator`] initialized with the current
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
    /// - `time`: A [`TimeSource`] implementation (e.g., [`CachedClock`])
    ///   that determines how timestamps are generated.
    ///
    /// # Returns
    ///
    /// A new [`AtomicSnowflakeGenerator`] ready to produce unique, time-ordered
    /// IDs.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "std")]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{CachedClock, TWITTER_EPOCH},
    /// };
    ///
    /// let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
    /// let generator = AtomicSnowflakeGenerator::new(0, clock);
    ///
    /// let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    ///
    /// [`TimeSource`]: crate::time::TimeSource
    /// [`CachedClock`]: crate::time::CachedClock
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
    /// - `timestamp`: The initial timestamp component (usually in time-source units)
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
        let initial = ID::from_components(timestamp, machine_id, sequence);
        Self {
            state: crossbeam_utils::CachePadded::new(AtomicU64::new(initial.to_raw())),
            time,
            _id: PhantomData,
        }
    }

    /// Generates a new ID.
    ///
    /// Returns a new, time-ordered, unique ID.
    ///
    /// When generation is pending, `f` receives a suggested [`Duration`],
    /// with clock granularity already applied. The delay may be zero and
    /// saturates at `u64::MAX` milliseconds.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "std")]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = AtomicSnowflakeGenerator::new(0, CachedClock::default());
    ///
    /// let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    pub fn next_id(&self, f: impl FnMut(Duration)) -> ID {
        SnowflakeGenerator::next_id(self, f)
    }

    /// Attempts to generate the next available ID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the sequence is full and the
    /// time has not advanced, or CAS fails), it returns [`Poll::Pending`].
    ///
    /// Pending delays are [`Duration`] values with clock granularity already
    /// applied. [`Duration::ZERO`] permits an immediate retry.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "std")]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{CachedClock, TWITTER_EPOCH},
    /// };
    ///
    /// let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
    /// let generator = AtomicSnowflakeGenerator::new(0, clock);
    ///
    /// let id: SnowflakeTwitterId = loop {
    ///     match generator.poll_id() {
    ///         Poll::Ready { id } => break id,
    ///         Poll::Pending { .. } => std::thread::yield_now(),
    ///     }
    /// };
    /// # }
    /// ```
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn poll_id(&self) -> Poll<ID> {
        let now = self.time.current_millis();

        let current_raw = self.state.load(Ordering::Relaxed);
        let current_id = ID::from_raw(current_raw);
        let current_ts = current_id.timestamp();

        let next_id = match now.cmp(&current_ts) {
            cmp::Ordering::Equal => {
                if current_id.has_sequence_room() {
                    current_id.increment_sequence()
                } else {
                    return Poll::Pending {
                        yield_for: Duration::from_millis(T::GRANULARITY_MILLIS.get()),
                    };
                }
            }
            cmp::Ordering::Greater => current_id.rollover_to_timestamp(now),
            cmp::Ordering::Less => {
                return Self::cold_clock_behind(now, current_ts);
            }
        };

        let next_raw = next_id.to_raw();

        if self
            .state
            .compare_exchange(current_raw, next_raw, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            Poll::Ready { id: next_id }
        } else {
            // CAS failed - another thread won the race. Yield 0 to retry
            // immediately.
            Poll::Pending {
                yield_for: Duration::ZERO,
            }
        }
    }

    #[cold]
    #[inline(never)]
    fn cold_clock_behind(now: ID::Ty, current_ts: ID::Ty) -> Poll<ID> {
        let units = current_ts - now;
        debug_assert!(units >= ID::ZERO);
        Poll::Pending {
            yield_for: Duration::from_millis(
                units.to_u64().saturating_mul(T::GRANULARITY_MILLIS.get()),
            ),
        }
    }
}

impl<ID, T> SnowflakeGenerator<ID, T> for AtomicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId<Ty = u64>,
    T: TimeSource<u64>,
{
    type Err = core::convert::Infallible;

    fn new(machine_id: ID::Ty, time: T) -> Self {
        Self::new(machine_id, time)
    }

    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err> {
        Ok(self.poll_id())
    }
}
