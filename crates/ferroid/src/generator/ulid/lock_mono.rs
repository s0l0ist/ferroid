use crate::{
    rand::RandSource, Error, IdGenStatus, Mutex, Result, TimeSource, UlidGenerator, UlidId,
};
use alloc::sync::Arc;
use core::cmp::Ordering;
#[cfg(feature = "tracing")]
use tracing::instrument;

/// A lock-based *monotonic* ULID-style ID generator suitable for multi-threaded
/// environments.
///
/// This generator wraps the Ulid state in an [`Arc<Mutex<_>>`], allowing safe
/// shared use across threads.
///
/// ## Features
/// - ✅ Thread-safe
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (monotonically increasing per millisecond)
///
/// ## Recommended When
/// - You're in a multi-threaded environment
/// - You need require monotonically increasing IDs (ID generated within the
///   same millisecond increment a sequence counter)
/// - Your target doesn't support atomics.
///
/// ## See Also
/// - [`BasicUlidGenerator`]
/// - [`BasicMonoUlidGenerator`]
/// - [`AtomicMonoUlidGenerator`]
///
/// [`BasicUlidGenerator`]: crate::BasicUlidGenerator
/// [`BasicMonoUlidGenerator`]: crate::BasicMonoUlidGenerator
/// [`AtomicMonoUlidGenerator`]: crate::AtomicMonoUlidGenerator
pub struct LockMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    #[cfg(feature = "cache-padded")]
    state: Arc<crossbeam_utils::CachePadded<Mutex<ID>>>,
    #[cfg(not(feature = "cache-padded"))]
    state: Arc<Mutex<ID>>,
    time: T,
    rng: R,
}

impl<ID, T, R> LockMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// Creates a new [`LockMonoUlidGenerator`] with the provided time source and
    /// RNG.
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
    /// use ferroid::{LockMonoUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let generator = LockMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = loop {
    ///     match generator.next_id() {
    ///         IdGenStatus::Ready { id } => break id,
    ///         IdGenStatus::Pending { .. } => core::hint::spin_loop(),
    ///     }
    /// };
    /// ```
    ///
    /// [`TimeSource`]: crate::TimeSource
    /// [`RandSource`]: crate::RandSource
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
    /// - `timestamp`: The initial timestamp component (usually in milliseconds)
    /// - `machine_id`: The machine or worker identifier
    /// - `sequence`: The initial sequence number
    /// - `time`: A [`TimeSource`] implementation used to fetch the current
    ///   time
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
            #[cfg(feature = "cache-padded")]
            state: Arc::new(crossbeam_utils::CachePadded::new(Mutex::new(id))),
            #[cfg(not(feature = "cache-padded"))]
            state: Arc::new(Mutex::new(id)),
            time,
            rng,
        }
    }

    /// Generates a new ULID.
    ///
    /// Internally calls [`Self::try_next_id`] and unwraps the result. This
    /// method will panic on error, so prefer the fallible version if you want
    /// explicit control over error handling.
    ///
    /// # Panics
    /// Panics if the lock is poisoned. For explicitly fallible behavior, use
    /// [`Self::try_next_id`] instead.
    ///
    /// # Example
    /// ```
    /// use ferroid::{LockMonoUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let generator = LockMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = loop {
    ///     match generator.next_id() {
    ///         IdGenStatus::Ready { id } => break id,
    ///         IdGenStatus::Pending { .. } => std::thread::yield_now(),
    ///     }
    /// };
    /// ```
    pub fn next_id(&self) -> IdGenStatus<ID> {
        self.try_next_id().unwrap()
    }

    /// Attempts to generate a new ULID with fallible error handling.
    ///
    /// Combines the current timestamp with a freshly generated random value to
    /// produce a unique identifier. Returns [`IdGenStatus::Ready`] on success.
    ///
    /// # Returns
    /// - `Ok(IdGenStatus::Ready { id })`: A new ID is available
    /// - `Ok(IdGenStatus::Pending { yield_for })`: The time to wait (in
    ///   milliseconds) before trying again
    /// - `Err(e)`: the lock was poisoned
    ///
    /// # Errors
    /// - Returns an error if the underlying lock has been poisoned.
    ///
    /// # Example
    /// ```
    /// use ferroid::{LockMonoUlidGenerator, IdGenStatus, ULID, ToU64, MonotonicClock, ThreadRandom};
    ///
    /// let generator = LockMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// // Attempt to generate a new ID
    /// let id: ULID = loop {
    ///     match generator.try_next_id() {
    ///         Ok(IdGenStatus::Ready { id }) => break id,
    ///         Ok(IdGenStatus::Pending { yield_for }) => {
    ///             std::thread::sleep(core::time::Duration::from_millis(yield_for.to_u64()));
    ///         }
    ///         Err(e) => panic!("Generator error: {}", e),
    ///     }
    /// };
    /// ```
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_next_id(&self) -> Result<IdGenStatus<ID>, Error<core::convert::Infallible>> {
        let now = self.time.current_millis();

        #[cfg(feature = "parking-lot")]
        let mut id = self.state.lock();
        #[cfg(not(feature = "parking-lot"))]
        let mut id = self.state.lock()?;

        let current_ts = id.timestamp();

        match now.cmp(&current_ts) {
            Ordering::Equal => {
                if id.has_random_room() {
                    *id = id.increment_random();
                    Ok(IdGenStatus::Ready { id: *id })
                } else {
                    Ok(IdGenStatus::Pending { yield_for: ID::ONE })
                }
            }
            Ordering::Greater => {
                let rand = self.rng.rand();
                *id = id.rollover_to_timestamp(now, rand);
                Ok(IdGenStatus::Ready { id: *id })
            }
            Ordering::Less => Ok(Self::cold_clock_behind(now, current_ts)),
        }
    }

    #[cold]
    #[inline(never)]
    fn cold_clock_behind(now: ID::Ty, current_ts: ID::Ty) -> IdGenStatus<ID> {
        let yield_for = current_ts - now;
        debug_assert!(yield_for >= ID::ZERO);
        IdGenStatus::Pending { yield_for }
    }
}

impl<ID, T, R> UlidGenerator<ID, T, R> for LockMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    type Err = Error;

    fn new(time: T, rng: R) -> Self {
        Self::new(time, rng)
    }

    fn next_id(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next_id(&self) -> Result<IdGenStatus<ID>, Self::Err> {
        self.try_next_id()
    }
}
