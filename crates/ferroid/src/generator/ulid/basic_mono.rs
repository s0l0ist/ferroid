use crate::{rand::RandSource, IdGenStatus, Result, TimeSource, UlidGenerator, UlidId};
use core::{cell::Cell, cmp::Ordering};
#[cfg(feature = "tracing")]
use tracing::instrument;

/// A *monotonic* ULID-style ID generator suitable for single-threaded
/// environments.
///
/// This generator is lightweight and fast, but is not thread-safe.
///
/// ## Features
/// - ❌ Not thread-safe
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (monotonically increasing per millisecond)
///
/// ## Recommended When
/// - You're in a single-threaded environment (no shared access)
/// - You need require monotonically increasing IDs (ID generated within the
///   same millisecond increment a sequence counter)
///
/// ## See Also
/// - [`BasicUlidGenerator`]
/// - [`LockMonoUlidGenerator`]
/// - [`AtomicMonoUlidGenerator`]
///
/// [`BasicUlidGenerator`]: crate::BasicUlidGenerator
/// [`AtomicMonoUlidGenerator`]: crate::AtomicMonoUlidGenerator
/// [`LockMonoUlidGenerator`]: crate::LockMonoUlidGenerator
pub struct BasicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    state: Cell<ID>,
    time: T,
    rng: R,
}

impl<ID, T, R> BasicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// Creates a new [`BasicMonoUlidGenerator`] with the provided time source and
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
    /// use ferroid::{BasicMonoUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let generator = BasicMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
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
            state: Cell::new(id),
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
    /// This method currently has no fallible code paths, but may panic if an
    /// internal error occurs in future implementations. For explicitly fallible
    /// behavior, use [`Self::try_next_id`] instead.
    ///
    /// # Example
    /// ```
    /// use ferroid::{BasicMonoUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let generator = BasicMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
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
    /// - `Err(_)`: infallible for this generator
    ///
    /// # Errors
    /// - This method currently does not return any errors and always returns
    ///   `Ok`. It is marked as fallible to allow for future extensibility
    ///
    /// # Example
    /// ```
    /// use ferroid::{BasicMonoUlidGenerator, IdGenStatus, ULID, ToU64, MonotonicClock, ThreadRandom};
    ///
    /// let generator = BasicMonoUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// // Attempt to generate a new ID
    /// let id: ULID = loop {
    ///     match generator.try_next_id() {
    ///         Ok(IdGenStatus::Ready { id }) => break id,
    ///         Ok(IdGenStatus::Pending { yield_for }) => {
    ///             std::thread::sleep(core::time::Duration::from_millis(yield_for.to_u64()));
    ///         }
    ///         Err(_) => unreachable!(),
    ///     }
    /// };
    /// ```
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_next_id(&self) -> Result<IdGenStatus<ID>> {
        let now = self.time.current_millis();
        let state = self.state.get();
        let current_ts = state.timestamp();

        match now.cmp(&current_ts) {
            Ordering::Equal => {
                if state.has_random_room() {
                    let updated = state.increment_random();
                    self.state.set(updated);
                    Ok(IdGenStatus::Ready { id: updated })
                } else {
                    Ok(IdGenStatus::Pending { yield_for: ID::ONE })
                }
            }
            Ordering::Greater => {
                // Set the new timestamp and random number.
                let rand = self.rng.rand();
                let updated = state.rollover_to_timestamp(now, rand);
                self.state.set(updated);
                Ok(IdGenStatus::Ready { id: updated })
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

impl<ID, T, R> UlidGenerator<ID, T, R> for BasicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    type Err = core::convert::Infallible;

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
