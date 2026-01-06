use core::{cmp, marker::PhantomData};

use portable_atomic::{AtomicU128, Ordering};
#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{IdGenStatus, Result, UlidGenerator},
    id::UlidId,
    rand::RandSource,
    time::TimeSource,
};

/// A lock-free *monotonic* ULID-style ID generator suitable for single-threaded
/// environments.
///
/// This generator stores the ULID in an [`AtomicU128`], allowing safe shared
/// use across threads.
///
/// ## Features
/// - ✅ Thread-safe
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (monotonically increasing per millisecond)
///
/// ## Caveats
/// This implementation uses an [`AtomicU128`] internally, so it only supports
/// ID layouts where the underlying type is [`u128`]. You cannot use layouts
/// with larger or smaller representations (i.e., `ID::Ty` must be [`u128`]).
///
/// ## Recommended When
/// - You're in a multi-threaded environment
/// - You need require monotonically increasing IDs (ID generated within the
///   same millisecond increment a sequence counter)
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
    #[cfg(feature = "cache-padded")]
    state: crossbeam_utils::CachePadded<AtomicU128>,
    #[cfg(not(feature = "cache-padded"))]
    state: AtomicU128,
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
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, IdGenStatus},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator =
    ///     AtomicMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = generator.next_id(|_| std::thread::yield_now());
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
    pub fn from_components(timestamp: ID::Ty, random: ID::Ty, time: T, rng: R) -> Self {
        let id = ID::from_components(timestamp, random);
        Self {
            #[cfg(feature = "cache-padded")]
            state: crossbeam_utils::CachePadded::new(AtomicU128::new(id.to_raw())),
            #[cfg(not(feature = "cache-padded"))]
            state: AtomicU128::new(id.to_raw()),
            time,
            rng,
            _id: PhantomData,
        }
    }

    /// Generates a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID.
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, IdGenStatus},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator =
    ///     AtomicMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = generator.next_id(|_| std::thread::yield_now());
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

    /// Generates a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID with fallible error handling.
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, IdGenStatus},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator =
    ///     AtomicMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = match generator.try_next_id(|_| std::thread::yield_now()) {
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
                IdGenStatus::Ready { id } => break Ok(id),
                IdGenStatus::Pending { yield_for } => f(yield_for),
            }
        }
    }

    /// Attempts to generate a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the sequence is full and the
    /// time has not advanced, or CAS fails), it returns
    /// [`IdGenStatus::Pending`].
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, IdGenStatus},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator =
    ///     AtomicMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = loop {
    ///     match generator.poll_id() {
    ///         IdGenStatus::Ready { id } => break id,
    ///         IdGenStatus::Pending { .. } => std::thread::yield_now(),
    ///     }
    /// };
    /// ```
    pub fn poll_id(&self) -> IdGenStatus<ID> {
        match self.try_poll_id() {
            Ok(id) => id,
            Err(e) =>
            {
                #[allow(unreachable_code)]
                match e {}
            }
        }
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
    /// - `Err(_)`: infallible for this generator
    ///
    /// # Errors
    /// - This method currently does not return any errors and always returns
    ///   `Ok`. It is marked as fallible to allow for future extensibility
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, IdGenStatus},
    ///     id::{ToU64, ULID},
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator =
    ///     AtomicMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// // Attempt to generate a new ID
    /// let id: ULID = loop {
    ///     match generator.try_poll_id() {
    ///         Ok(IdGenStatus::Ready { id }) => break id,
    ///         Ok(IdGenStatus::Pending { yield_for }) => {
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
    pub fn try_poll_id(&self) -> Result<IdGenStatus<ID>> {
        let now = self.time.current_millis();

        let current_raw = self.state.load(Ordering::Relaxed);
        let current_id = ID::from_raw(current_raw);
        let current_ts = current_id.timestamp();

        let next_id = match now.cmp(&current_ts) {
            cmp::Ordering::Equal => {
                if current_id.has_random_room() {
                    current_id.increment_random()
                } else {
                    return Ok(IdGenStatus::Pending { yield_for: ID::ONE });
                }
            }
            cmp::Ordering::Greater => current_id.rollover_to_timestamp(now, self.rng.rand()),
            cmp::Ordering::Less => {
                return Ok(Self::cold_clock_behind(now, current_ts));
            }
        };

        let next_raw = next_id.to_raw();

        if self
            .state
            .compare_exchange(current_raw, next_raw, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            Ok(IdGenStatus::Ready { id: next_id })
        } else {
            // CAS failed - another thread won the race. Yield 0 to retry
            // immediately.
            Ok(IdGenStatus::Pending {
                yield_for: ID::ZERO,
            })
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

    fn next_id(&self, f: impl FnMut(ID::Ty)) -> ID {
        self.next_id(f)
    }

    fn try_next_id(&self, f: impl FnMut(ID::Ty)) -> Result<ID, Self::Err> {
        self.try_next_id(f)
    }

    fn poll_id(&self) -> IdGenStatus<ID> {
        self.poll_id()
    }

    fn try_poll_id(&self) -> Result<IdGenStatus<ID>, Self::Err> {
        self.try_poll_id()
    }
}
