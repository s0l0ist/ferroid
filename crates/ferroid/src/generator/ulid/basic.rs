use crate::{IdGenStatus, Result, TimeSource, Ulid, UlidGenerator, rand::RandSource};
use core::marker::PhantomData;
#[cfg(feature = "tracing")]
use tracing::instrument;

/// A ULID-style ID generator suitable for single- or multi-threaded
/// environments.
///
/// This generator is lightweight and fast. Thread safety depends entirely on
/// the [`TimeSource`] and [`RandSource`] used. The provided defaults
/// (`MonotonicClock` and [`ThreadRandom`]) are thread-safe and allow safe
/// concurrent use.
///
/// Unlike Snowflake generators, this type avoids sequence coordination entirely
/// by combining a timestamp with random bits to produce probabilistically
/// unique, time-sortable IDs.
///
/// ## Features
///
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (millisecond precision)
/// - ✅ Suitable for distributed systems
/// - ✅ No sequence exhaustion or clock rollback issues
///
/// ## Recommended When
/// - You want high-performance, distributed-safe IDs
/// - You need time-ordering but can tolerate approximate monotonicity
/// - You want to avoid shared state, locking, or sequence contention
///
/// ## Trade-offs
///
/// - **Uniqueness**: Probabilistic (collision risk ≈ 1 in 2^random_bits)
/// - **Performance**: Extremely fast (no locking, no sequencing)
/// - **Ordering**: Time-ordered but not strictly monotonic
/// - **Coordination**: None required between generators
///
/// ## Thread Safety
///
/// This generator is **only as thread-safe as its `TimeSource` and
/// `RandSource`**. The default [`ThreadRandom`] implementation uses
/// `thread_rng()` internally, making it safe for concurrent use across threads
/// without contention.
///
/// [`ThreadRandom`]: crate::ThreadRandom
/// [`TimeSource`]: crate::TimeSource
/// [`RandSource`]: crate::rand::RandSource
pub struct BasicUlidGenerator<ID, T, R>
where
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    clock: T,
    rng: R,
    _id: PhantomData<ID>,
}

impl<ID, T, R> BasicUlidGenerator<ID, T, R>
where
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// Creates a new [`BasicUlidGenerator`] with the provided time source and
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
    /// use ferroid::{BasicUlidGenerator, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let generator = BasicUlidGenerator::<ULID, _, _>::new(MonotonicClock::default(), ThreadRandom::default());
    /// let id = generator.next_id();
    /// println!("Generated ID: {:?}", id);
    /// ```
    ///
    /// [`TimeSource`]: crate::TimeSource
    /// [`RandSource`]: crate::RandSource
    pub fn new(clock: T, rng: R) -> Self {
        Self {
            clock,
            rng,
            _id: PhantomData,
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
    /// use ferroid::{BasicUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let clock = MonotonicClock::default();
    /// let rand = ThreadRandom::default();
    /// let generator = BasicUlidGenerator::<ULID, _, _>::new(clock, rand);
    ///
    /// // Attempt to generate a new ID
    /// match generator.next_id() {
    ///     IdGenStatus::Ready { id } => {
    ///         println!("ID: {}", id);
    ///     }
    ///     IdGenStatus::Pending { yield_for } => {
    ///         // In practice, Ulid generators will never return `Pending`, but
    ///         // it is kept to have a consistent API.
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
    /// # Example
    /// ```
    /// use ferroid::{BasicUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    /// let clock = MonotonicClock::default();
    /// let rand = ThreadRandom::default();
    /// let generator = BasicUlidGenerator::<ULID, _, _>::new(clock, rand);
    ///
    /// // Attempt to generate a new ID
    /// match generator.try_next_id() {
    ///     Ok(IdGenStatus::Ready { id }) => {
    ///         println!("ID: {}", id);
    ///     }
    ///     Ok(IdGenStatus::Pending { yield_for }) => {
    ///         // In practice, Ulid generators will never return `Pending`, but
    ///         // it is kept to have a consistent API.
    ///         println!("Exhausted for: {}ms", yield_for);
    ///     }
    ///     Err(e) => eprintln!("Generator error: {}", e),
    /// }
    /// ```
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_next_id(&self) -> Result<IdGenStatus<ID>> {
        let t = self.clock.current_millis();
        let r = self.rng.rand();
        Ok(IdGenStatus::Ready {
            id: ID::from_components(t, r),
        })
    }
}

impl<ID, T, R> UlidGenerator<ID, T, R> for BasicUlidGenerator<ID, T, R>
where
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    fn new(clock: T, rng: R) -> Self {
        Self::new(clock, rng)
    }

    fn next_id(&self) -> IdGenStatus<ID> {
        self.next_id()
    }

    fn try_next_id(&self) -> Result<IdGenStatus<ID>> {
        self.try_next_id()
    }
}
