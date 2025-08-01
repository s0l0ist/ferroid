use crate::{Error, IdGenStatus, Result, TimeSource, Ulid, UlidGenerator, rand::RandSource};
use core::cmp::Ordering;
use std::sync::{Arc, Mutex};
#[cfg(feature = "tracing")]
use tracing::instrument;

/// A *monotonic* ULID-style ID generator suitable for multi-threaded
/// environments.
///
/// This generator wraps the Ulid state in an [`Arc<Mutex<_>>`], allowing safe
/// shared use across threads.
///
/// ## Features
///
/// - ✅ Thread-safe
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (monotonically increasing per millisecond)
///
/// ## Recommended When
///
/// - You're in a multi-threaded environment
/// - You need require monotonically increasing IDs (ID generated within the
///   same millisecond increment a sequence counter)
///
/// ## See Also
/// - [`BasicUlidGenerator`]
///
/// [`BasicUlidGenerator`]: crate::BasicUlidGenerator
pub struct LockUlidGenerator<ID, T, R>
where
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    state: Arc<Mutex<ID>>,
    clock: T,
    rng: R,
}

impl<ID, T, R> LockUlidGenerator<ID, T, R>
where
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// Creates a new [`LockUlidGenerator`] with the provided time source and
    /// RNG.
    ///
    /// # Parameters
    /// - `clock`: A [`TimeSource`] used to retrieve the current timestamp
    /// - `rng`: A [`RandSource`] used to generate random bits
    ///
    /// # Returns
    /// A ready-to-use ULID generator suitable for producing unique, sortable
    /// IDs.
    ///
    /// # Example
    /// ```
    /// use ferroid::{LockUlidGenerator, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let generator = LockUlidGenerator::<ULID, _, _>::new(MonotonicClock::default(), ThreadRandom::default());
    /// let id = generator.next_id();
    /// println!("Generated ID: {:?}", id);
    /// ```
    ///
    /// [`TimeSource`]: crate::TimeSource
    /// [`RandSource`]: crate::RandSource
    pub fn new(clock: T, rng: R) -> Self {
        Self::from_components(ID::ZERO, ID::ZERO, clock, rng)
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
    /// - `clock`: A [`TimeSource`] implementation used to fetch the current
    ///   time
    ///
    /// # Returns
    /// A new generator instance preloaded with the given state.
    ///
    /// # ⚠️ Note
    /// In typical use cases, you should prefer [`Self::new`] to let the
    /// generator initialize itself from the current time.
    pub fn from_components(timestamp: ID::Ty, random: ID::Ty, clock: T, rng: R) -> Self {
        let id = ID::from_components(timestamp, random);
        Self {
            state: Arc::new(Mutex::new(id)),
            clock,
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
    /// use ferroid::{LockUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let clock = MonotonicClock::default();
    /// let rand = ThreadRandom::default();
    /// let generator = LockUlidGenerator::<ULID, _, _>::new(clock, rand);
    ///
    /// // Attempt to generate a new ID
    /// match generator.next_id() {
    ///     IdGenStatus::Ready { id } => {
    ///         println!("ID: {}", id);
    ///     }
    ///     IdGenStatus::Pending { yield_for } => {
    ///         println!("Exhausted; wait for: {}ms", yield_for);
    ///     }
    /// }
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
    /// - Ok(IdGenStatus::Ready { id }) A ULID was generated
    /// - Ok(IdGenStatus::Pending { yield_for }) Never, but kept to match the
    ///   Snowflake API
    /// - Err(e) if the time source or rand source failed
    ///
    /// # Errors
    /// - Returns an error if the underlying lock has been poisoned.
    ///
    /// # Example
    /// ```
    /// use ferroid::{LockUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let clock = MonotonicClock::default();
    /// let rand = ThreadRandom::default();
    /// let generator = LockUlidGenerator::<ULID, _, _>::new(clock, rand);
    ///
    /// // Attempt to generate a new ID
    /// match generator.try_next_id() {
    ///     Ok(IdGenStatus::Ready { id }) => {
    ///         println!("ID: {}", id);
    ///     }
    ///     Ok(IdGenStatus::Pending { yield_for }) => {
    ///         // In practice, Ulid generators will never return `Pending`, but
    ///         // it is kept to have a consistent API.
    ///         println!("Exhausted; wait for: {}ms", yield_for);
    ///     }
    ///     Err(e) => eprintln!("Generator error: {}", e),
    /// }
    /// ```
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_next_id(&self) -> Result<IdGenStatus<ID>, Error<core::convert::Infallible>> {
        let now = self.clock.current_millis();
        let mut id = self.state.lock()?;
        let current_ts = id.timestamp();

        let status = match now.cmp(&current_ts) {
            Ordering::Less => {
                let yield_for = current_ts - now;
                debug_assert!(yield_for >= ID::ZERO);
                IdGenStatus::Pending { yield_for }
            }
            Ordering::Greater => {
                let rand = self.rng.rand();
                *id = id.rollover_to_timestamp(now, rand);
                IdGenStatus::Ready { id: *id }
            }
            Ordering::Equal => {
                if id.has_random_room() {
                    *id = id.increment_random();
                    IdGenStatus::Ready { id: *id }
                } else {
                    IdGenStatus::Pending { yield_for: ID::ONE }
                }
            }
        };

        Ok(status)
    }
}

impl<ID, T, R> UlidGenerator<ID, T, R> for LockUlidGenerator<ID, T, R>
where
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    type Err = Error;

    fn new(clock: T, rng: R) -> Self {
        Self::new(clock, rng)
    }

    fn next_id(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next_id(&self) -> Result<IdGenStatus<ID>, Self::Err> {
        self.try_next_id()
    }
}
