use crate::{IdGenStatus, Result, Snowflake, TimeSource};
use std::{
    cmp,
    marker::PhantomData,
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(feature = "tracing")]
use tracing::instrument;

/// A lock-free Snowflake ID generator suitable for multi-threaded environments.
///
/// This generator stores the Snowflake state in an [`AtomicU64`], allowing safe
/// shared use across threads.
///
/// ## Features
///
/// - ✅ Thread-safe
/// - ❌ Safely implement any [`Snowflake`] layout
///
/// ## Caveats
/// This implementation uses an `AtomicU64` internally, so it only supports ID
/// layouts where the underlying type is `u64`. You cannot use layouts with
/// larger or smaller representations (i.e., `ID::Ty` must be `u64`).
///
/// ## Recommended When
/// - You're in a multi-threaded environment
/// - You want the lowest possible latency under light-to-moderate contention
///
/// ## See Also
/// - [`BasicSnowflakeGenerator`]
/// - [`LockSnowflakeGenerator`]
///
/// [`BasicSnowflakeGenerator`]: crate::BasicSnowflakeGenerator
/// [`LockSnowflakeGenerator`]: crate::LockSnowflakeGenerator
pub struct AtomicSnowflakeGenerator<ID, T>
where
    ID: Snowflake<Ty = u64>,
    T: TimeSource<ID::Ty>,
{
    state: AtomicU64,
    machine_id: u64,
    time: T,
    _id: PhantomData<ID>,
}

impl<ID, T> AtomicSnowflakeGenerator<ID, T>
where
    ID: Snowflake<Ty = u64>,
    T: TimeSource<ID::Ty>,
{
    /// Creates a new [`AtomicSnowflakeGenerator`] initialized with the current
    /// time and a given machine ID.
    ///
    /// This constructor sets the initial timestamp and sequence to zero, and
    /// uses the provided `clock` to fetch the current time during ID
    /// generation. It is the recommended way to create a new atomic generator
    /// for typical use cases.
    ///
    /// # Parameters
    ///
    /// - `machine_id`: A unique identifier for the node or instance generating
    ///   IDs. This value will be encoded into every generated ID.
    /// - `clock`: A [`TimeSource`] implementation (e.g., [`MonotonicClock`])
    ///   that determines how timestamps are generated.
    ///
    /// # Returns
    ///
    /// A new [`AtomicSnowflakeGenerator`] ready to produce unique, time-ordered
    /// IDs.
    ///
    /// # Example
    ///
    /// ```
    /// use ferroid::{AtomicSnowflakeGenerator, SnowflakeTwitterId, MonotonicClock};
    ///
    /// let generator = AtomicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, MonotonicClock::default());
    /// let id = generator.next_id();
    /// ```
    ///
    /// [`TimeSource`]: crate::TimeSource
    /// [`MonotonicClock`]: crate::MonotonicClock
    pub fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::from_components(ID::ZERO, machine_id, ID::ZERO, clock)
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
    /// # Note
    /// In typical use cases, you should prefer [`Self::new`] to let the
    /// generator initialize itself from the current time.
    pub fn from_components(
        timestamp: ID::Ty,
        machine_id: ID::Ty,
        sequence: ID::Ty,
        clock: T,
    ) -> Self {
        let initial = ID::from_components(timestamp, machine_id, sequence);
        Self {
            state: AtomicU64::new(initial.to_raw()),
            machine_id,
            time: clock,
            _id: PhantomData,
        }
    }

    /// Attempts to generate the next available ID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the sequence is full and the
    /// clock has not advanced), it returns [`IdGenStatus::Pending`].
    ///
    /// # Panics
    /// This method currently has no fallible code paths, but may panic if an
    /// internal error occurs in future implementations. For explicitly fallible
    /// behavior, use [`Self::try_next_id`] instead.
    ///
    /// # Example
    /// ```
    /// use ferroid::{AtomicSnowflakeGenerator, SnowflakeTwitterId, IdGenStatus, MonotonicClock, TimeSource};
    ///
    /// // Create a clock and a generator with machine_id = 0
    /// let clock = MonotonicClock::default();
    /// let mut generator = AtomicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, clock);
    ///
    /// // Attempt to generate a new ID
    /// match generator.next_id() {
    ///     IdGenStatus::Ready { id } => {
    ///         println!("ID: {}", id);
    ///         assert_eq!(id.machine_id(), 0);
    ///     }
    ///     IdGenStatus::Pending { yield_for } => {
    ///         // This should rarely happen on the first call, but if it does,
    ///         // backoff or yield and try again.
    ///         println!("Exhausted; wait until: {}", yield_for);
    ///     }
    /// }
    /// ```
    pub fn next_id(&self) -> IdGenStatus<ID> {
        self.try_next_id().unwrap()
    }

    /// A fallible version of [`Self::next_id`] that returns a [`Result`].
    ///
    /// This method attempts to generate the next ID based on the current time
    /// and internal state. If successful, it returns [`IdGenStatus::Ready`]
    /// with a newly generated ID. If the generator is temporarily exhausted, it
    /// returns [`IdGenStatus::Pending`]. If an internal failure occurs (e.g., a
    /// time source or lock error), it returns an error.
    ///
    /// # Returns
    /// - `Ok(IdGenStatus::Ready { id })`: A new ID is available
    /// - `Ok(IdGenStatus::Pending { yield_for })`: Wait for time (in milliseconds) to advance
    /// - `Err(e)`: A recoverable error occurred (e.g., time source failure)
    ///
    /// # Example
    /// ```
    /// use ferroid::{AtomicSnowflakeGenerator, SnowflakeTwitterId, IdGenStatus, MonotonicClock, TimeSource};
    ///
    /// // Create a clock and a generator with machine_id = 0
    /// let clock = MonotonicClock::default();
    /// let mut generator = AtomicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, clock);
    ///
    /// // Attempt to generate a new ID
    /// match generator.try_next_id() {
    ///     Ok(IdGenStatus::Ready { id }) => {
    ///         println!("ID: {}", id);
    ///         assert_eq!(id.machine_id(), 0);
    ///     }
    ///     Ok(IdGenStatus::Pending { yield_for }) => {
    ///         println!("Exhausted; wait until: {}", yield_for);
    ///     }
    ///     Err(err) => eprintln!("Generator error: {}", err),
    /// }
    /// ```
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_next_id(&self) -> Result<IdGenStatus<ID>> {
        let now = self.time.current_millis();

        let current_raw = self.state.load(Ordering::Relaxed);
        let current_id = ID::from_raw(current_raw);

        let current_ts = current_id.timestamp();
        let seq = current_id.sequence();

        let (next_ts, next_seq) = match now.cmp(&current_ts) {
            cmp::Ordering::Less => {
                let yield_for = current_ts - now;
                debug_assert!(yield_for >= ID::ZERO);
                return Ok(IdGenStatus::Pending { yield_for });
            }
            cmp::Ordering::Greater => (now, ID::ZERO),
            cmp::Ordering::Equal => {
                if seq < ID::max_sequence() {
                    (current_ts, seq + ID::ONE)
                } else {
                    return Ok(IdGenStatus::Pending { yield_for: ID::ONE });
                }
            }
        };

        let next_id = ID::from_components(next_ts, self.machine_id, next_seq);
        let next_raw = next_id.to_raw();

        if self
            .state
            .compare_exchange(current_raw, next_raw, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            Ok(IdGenStatus::Ready { id: next_id })
        } else {
            // CAS failed — another thread won the race. Yield 0 to retry immediately.
            Ok(IdGenStatus::Pending {
                yield_for: ID::ZERO,
            })
        }
    }
}
