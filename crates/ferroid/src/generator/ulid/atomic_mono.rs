use core::{cmp, marker::PhantomData, time::Duration};

use portable_atomic::{AtomicU128, Ordering};
#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{Poll, Result, UlidGenerator},
    id::{ToU64, UlidId},
    rand::RandSource,
    time::TimeSource,
};

/// A lock-free *monotonic* ULID-style ID generator suitable for multi-threaded
/// environments.
///
/// This generator stores the ULID in an [`AtomicU128`], allowing safe shared
/// use across threads.
///
/// ## Features
/// - ✅ Thread-safe
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (monotonically increasing per time-source tick)
///
/// ## Caveats
/// This implementation uses an [`AtomicU128`] internally, so it only supports
/// ID layouts where the underlying type is [`u128`]. You cannot use layouts
/// with larger or smaller representations (i.e., `ID::Ty` must be [`u128`]).
///
/// ## Recommended When
/// - You're in a multi-threaded environment
/// - You need monotonically increasing IDs (IDs generated within the same
///   time-source tick increment the random component)
///
/// ## See Also
/// - [`BasicUlidGenerator`]
/// - [`BasicMonoUlidGenerator`]
/// - [`LockMonoUlidGenerator`]
///
/// [`BasicUlidGenerator`]: crate::generator::BasicUlidGenerator
/// [`BasicMonoUlidGenerator`]: crate::generator::BasicMonoUlidGenerator
/// [`LockMonoUlidGenerator`]: crate::generator::LockMonoUlidGenerator
pub struct AtomicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId<Ty = u128>,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    state: crossbeam_utils::CachePadded<AtomicU128>,
    time: T,
    rng: R,
    _id: PhantomData<ID>,
}

impl<ID, T, R> AtomicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId<Ty = u128>,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// Creates a new [`AtomicMonoUlidGenerator`] with the provided time source
    /// and RNG.
    ///
    /// # Parameters
    /// - `time`: A [`TimeSource`] used to retrieve the current timestamp
    /// - `rng`: A [`RandSource`] used to generate random bits
    ///
    /// # Returns
    /// A ready-to-use ULID generator suitable for producing unique, sortable
    /// IDs.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "thread-rng")]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = AtomicMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    ///
    /// [`TimeSource`]: crate::time::TimeSource
    /// [`RandSource`]: crate::rand::RandSource
    pub fn new(time: T, rng: R) -> Self {
        Self::from_components(ID::ZERO, ID::ZERO, time, rng)
    }

    /// Creates a new ID generator from explicit component values.
    ///
    /// This constructor is primarily useful for advanced use cases such as
    /// restoring state from persistent storage or controlling the starting
    /// point of the generator manually.
    ///
    /// # Parameters
    /// - `timestamp`: The initial timestamp component (usually in
    ///   time-source units)
    /// - `random`: The initial random component
    /// - `time`: A [`TimeSource`] implementation used to fetch the current time
    /// - `rng`: A [`RandSource`] used to generate future random bits
    ///
    /// # Returns
    /// A new generator instance preloaded with the given state.
    ///
    /// # ⚠️ Note
    /// In typical use cases, you should prefer [`Self::new`] to let the
    /// generator initialize itself from the current time.
    pub fn from_components(timestamp: ID::Ty, random: ID::Ty, time: T, rng: R) -> Self {
        let id = ID::from_components(timestamp, random);
        Self {
            state: crossbeam_utils::CachePadded::new(AtomicU128::new(id.to_raw())),
            time,
            rng,
            _id: PhantomData,
        }
    }

    /// Generates a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID.
    ///
    /// When generation is pending, `f` receives a suggested [`Duration`],
    /// with clock granularity already applied. The delay may be zero and
    /// saturates at `u64::MAX` milliseconds.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "thread-rng")]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = AtomicMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    pub fn next_id(&self, f: impl FnMut(Duration)) -> ID {
        UlidGenerator::next_id(self, f)
    }

    /// Attempts to generate a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the random component is
    /// exhausted and the time has not advanced, or CAS fails), it returns
    /// [`Poll::Pending`].
    ///
    /// Pending delays are [`Duration`] values with clock granularity already
    /// applied. [`Duration::ZERO`] permits an immediate retry.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "thread-rng")]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = AtomicMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = loop {
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
                if current_id.has_random_room() {
                    current_id.increment_random()
                } else {
                    return Poll::Pending {
                        yield_for: Duration::from_millis(T::GRANULARITY_MILLIS.get()),
                    };
                }
            }
            cmp::Ordering::Greater => current_id.rollover_to_timestamp(now, self.rng.rand()),
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

impl<ID, T, R> UlidGenerator<ID, T, R> for AtomicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId<Ty = u128>,
    T: TimeSource<u128>,
    R: RandSource<u128>,
{
    type Err = core::convert::Infallible;

    fn new(time: T, rng: R) -> Self {
        Self::new(time, rng)
    }

    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err> {
        Ok(self.poll_id())
    }
}
