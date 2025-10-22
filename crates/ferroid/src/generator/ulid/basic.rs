use core::marker::PhantomData;

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    Result,
    generator::{IdGenStatus, UlidGenerator},
    id::UlidId,
    rand::RandSource,
    time::TimeSource,
};

/// A *non-monotonic* ULID-style ID generator suitable for multi-threaded
/// environments.
///
/// This generator is lightweight and fast, but has a higher collision
/// probabiliy than it's monotonic counterpart.
///
/// ## Features
/// - ✅ Thread-safe
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (not monotonically increasing, random per millisecond)
///
/// ## Recommended When
/// - You're in a single or multi-threaded environment
/// - You require purely random IDs (even within the same millisecond)
///
/// ## See Also
/// - [`BasicMonoUlidGenerator`]
/// - [`LockMonoUlidGenerator`]
/// - [`AtomicMonoUlidGenerator`]
///
/// [`BasicMonoUlidGenerator`]: crate::generator::BasicMonoUlidGenerator
/// [`AtomicMonoUlidGenerator`]: crate::generator::AtomicMonoUlidGenerator
/// [`LockMonoUlidGenerator`]: crate::generator::LockMonoUlidGenerator
pub struct BasicUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    time: T,
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
    ///     generator::{BasicUlidGenerator, IdGenStatus},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = loop {
    ///     match generator.next_id() {
    ///         IdGenStatus::Ready { id } => break id,
    ///         IdGenStatus::Pending { .. } => core::hint::spin_loop(),
    ///     }
    /// };
    /// ```
    ///
    /// [`TimeSource`]: crate::time::TimeSource
    /// [`RandSource`]: crate::rand::RandSource
    pub const fn new(time: T, rng: R) -> Self {
        Self {
            time,
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
    /// use ferroid::{
    ///     generator::{BasicUlidGenerator, IdGenStatus},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
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
    /// use ferroid::{
    ///     generator::{BasicUlidGenerator, IdGenStatus},
    ///     id::{ToU64, ULID},
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
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
        Ok(IdGenStatus::Ready {
            id: ID::from_components(self.time.current_millis(), self.rng.rand()),
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
