use crate::{IdGenStatus, Result, TimeSource, UlidGenerator, UlidId, rand::RandSource};
use core::marker::PhantomData;
#[cfg(feature = "tracing")]
use tracing::instrument;

/// A *non-monotonic* ULID-style ID generator suitable for multi-threaded
/// environments.
///
/// This generator is lightweight and fast, but has a higher collision
/// probabiliy than it's monotonic counterpart.
///
/// ## Features
///
/// - ✅ Thread-safe
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (not monotonically increasing, random per millisecond)
///
/// ## Recommended When
///
/// - You're in a single or multi-threaded environment
/// - You require purely random IDs (even within the same millisecond)
///
/// ## See Also
/// - [`BasicMonoUlidGenerator`]
/// - [`LockMonoUlidGenerator`]
///
/// [`BasicMonoUlidGenerator`]: crate::BasicMonoUlidGenerator
/// [`LockMonoUlidGenerator`]: crate::LockMonoUlidGenerator
pub struct BasicUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    clock: T,
    rng: R,
    _id: PhantomData<ID>,
}

impl<ID, T, R> BasicUlidGenerator<ID, T, R>
where
    ID: UlidId,
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
    /// #[cfg(all(feature = "std", feature = "alloc", feature = "ulid"))]
    /// {
    ///     use ferroid::{BasicUlidGenerator, ULID, MonotonicClock, ThreadRandom};
    ///
    ///     let generator = BasicUlidGenerator::<ULID, _, _>::new(MonotonicClock::default(), ThreadRandom::default());
    ///     let id = generator.next_id();
    ///     println!("Generated ID: {:?}", id);
    /// }
    /// ```
    ///
    /// [`TimeSource`]: crate::TimeSource
    /// [`RandSource`]: crate::RandSource
    pub const fn new(clock: T, rng: R) -> Self {
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
    /// #[cfg(all(feature = "std", feature = "alloc", feature = "ulid"))]
    /// {
    ///     use ferroid::{BasicUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    ///     let clock = MonotonicClock::default();
    ///     let rand = ThreadRandom::default();
    ///     let generator = BasicUlidGenerator::<ULID, _, _>::new(clock, rand);
    ///
    ///     // Attempt to generate a new ID
    ///     match generator.next_id() {
    ///         IdGenStatus::Ready { id } => {
    ///             println!("ID: {}", id);
    ///         }
    ///         IdGenStatus::Pending { yield_for } => {
    ///             println!("Exhausted; wait for: {}ms", yield_for);
    ///         }
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
    /// - This method currently does not return any errors and always returns
    ///   `Ok`. It is marked as fallible to allow for future extensibility
    ///
    /// # Example
    /// ```
    /// #[cfg(all(feature = "std", feature = "alloc", feature = "ulid"))]
    /// {
    ///     use ferroid::{BasicUlidGenerator, IdGenStatus, ULID, MonotonicClock, ThreadRandom};
    ///
    ///     let clock = MonotonicClock::default();
    ///     let rand = ThreadRandom::default();
    ///     let generator = BasicUlidGenerator::<ULID, _, _>::new(clock, rand);
    ///
    ///     // Attempt to generate a new ID
    ///     match generator.try_next_id() {
    ///         Ok(IdGenStatus::Ready { id }) => {
    ///             println!("ID: {}", id);
    ///         }
    ///         Ok(IdGenStatus::Pending { yield_for }) => {
    ///             println!("Exhausted; wait for: {}ms", yield_for);
    ///         }
    ///         Err(e) => eprintln!("Generator error: {}", e),
    ///     }
    /// }
    /// ```
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_next_id(&self) -> Result<IdGenStatus<ID>> {
        Ok(IdGenStatus::Ready {
            id: ID::from_components(self.clock.current_millis(), self.rng.rand()),
        })
    }
}

impl<ID, T, R> UlidGenerator<ID, T, R> for BasicUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    type Err = core::convert::Infallible;

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
