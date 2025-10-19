use crate::{IdGenStatus, Result, SnowflakeGenerator, SnowflakeId, TimeSource};
use core::{cell::Cell, cmp::Ordering};
#[cfg(feature = "tracing")]
use tracing::instrument;

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
/// [`LockSnowflakeGenerator`]: crate::LockSnowflakeGenerator
/// [`AtomicSnowflakeGenerator`]: crate::AtomicSnowflakeGenerator
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
    /// A new [`BasicSnowflakeGenerator`] ready to produce unique, time-ordered
    /// IDs.
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(all(feature = "std", feature = "alloc", feature = "snowflake"))]
    /// {
    ///     use ferroid::{BasicSnowflakeGenerator, IdGenStatus, SnowflakeTwitterId, TWITTER_EPOCH, MonotonicClock};
    ///     
    ///     let generator = BasicSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
    ///
    ///     let id: SnowflakeTwitterId = loop {
    ///         match generator.next_id() {
    ///             IdGenStatus::Ready { id } => break id,
    ///             IdGenStatus::Pending { .. } => core::hint::spin_loop(),
    ///         }
    ///     };
    /// }
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
    /// # ⚠️ Note
    /// In typical use cases, you should prefer [`Self::new`] to let the
    /// generator initialize itself from the current time.
    pub fn from_components(
        timestamp: ID::Ty,
        machine_id: ID::Ty,
        sequence: ID::Ty,
        clock: T,
    ) -> Self {
        let id = ID::from_components(timestamp, machine_id, sequence);
        Self {
            state: Cell::new(id),
            time: clock,
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
    /// #[cfg(all(feature = "std", feature = "alloc", feature = "snowflake"))]
    /// {
    ///     use ferroid::{BasicSnowflakeGenerator, IdGenStatus, SnowflakeTwitterId, TWITTER_EPOCH, MonotonicClock};
    ///     
    ///     let generator = BasicSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
    ///
    ///     let id: SnowflakeTwitterId = loop {
    ///         match generator.next_id() {
    ///             IdGenStatus::Ready { id } => break id,
    ///             IdGenStatus::Pending { .. } => std::thread::yield_now(),
    ///         }
    ///     };
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
    /// #[cfg(all(feature = "std", feature = "alloc", feature = "snowflake"))]
    /// {
    ///     use ferroid::{BasicSnowflakeGenerator, ToU64, IdGenStatus, SnowflakeTwitterId, TWITTER_EPOCH, MonotonicClock};
    ///     
    ///     let generator = BasicSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
    ///
    ///     // Attempt to generate a new ID
    ///     let id: SnowflakeTwitterId = loop {
    ///         match generator.try_next_id() {
    ///             Ok(IdGenStatus::Ready { id }) => break id,
    ///             Ok(IdGenStatus::Pending { yield_for }) => {
    ///                 std::thread::sleep(core::time::Duration::from_millis(yield_for.to_u64()));
    ///             }
    ///             Err(_) => unreachable!(),
    ///         }
    ///     };
    /// }
    /// ```
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_next_id(&self) -> Result<IdGenStatus<ID>> {
        let now = self.time.current_millis();
        let state = self.state.get();
        let current_ts = state.timestamp();

        match now.cmp(&current_ts) {
            Ordering::Equal => {
                if state.has_sequence_room() {
                    let updated = state.increment_sequence();
                    self.state.set(updated);
                    Ok(IdGenStatus::Ready { id: updated })
                } else {
                    Ok(IdGenStatus::Pending { yield_for: ID::ONE })
                }
            }
            Ordering::Greater => {
                let updated = state.rollover_to_timestamp(now);
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

impl<ID, T> SnowflakeGenerator<ID, T> for BasicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err = core::convert::Infallible;

    fn new(machine_id: ID::Ty, clock: T) -> Self {
        Self::new(machine_id, clock)
    }

    fn next_id(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next_id(&self) -> Result<IdGenStatus<ID>, Self::Err> {
        self.try_next_id()
    }
}
