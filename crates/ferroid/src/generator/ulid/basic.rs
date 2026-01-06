use core::marker::PhantomData;

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{Poll, Result, UlidGenerator},
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
    ///     generator::{BasicUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = generator.next_id(|_| std::thread::yield_now());
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
    /// Returns a new, time-ordered, unique ID.
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{BasicUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
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
    ///     generator::{BasicUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
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
                Poll::Ready { id } => break Ok(id),
                Poll::Pending { yield_for } => f(yield_for),
            }
        }
    }

    /// Generates a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. A basic
    /// generator always generates new random bytes and doesn't need to yield.
    /// Therefore, we can safely assume it will always return `Poll::Ready`.
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{BasicUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = loop {
    ///     match generator.poll_id() {
    ///         Poll::Ready { id } => break id,
    ///         Poll::Pending { .. } => unreachable!(),
    ///     }
    /// };
    /// ```
    pub fn poll_id(&self) -> Poll<ID> {
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
    /// produce a unique identifier. Returns [`Poll::Ready`] on success.
    ///
    /// # Returns
    /// - `Ok(Poll::Ready { id })`: A new ID is available
    /// - `Ok(Poll::Pending { yield_for })`: The time to wait (in milliseconds)
    ///   before trying again
    /// - `Err(_)`: infallible for this generator
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{BasicUlidGenerator, Poll},
    ///     id::{ToU64, ULID},
    ///     rand::ThreadRandom,
    ///     time::MonotonicClock,
    /// };
    ///
    /// let generator = BasicUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
    ///
    /// // Attempt to generate a new ID
    /// let id: ULID = loop {
    ///     match generator.try_poll_id() {
    ///         Ok(Poll::Ready { id }) => break id,
    ///         Ok(Poll::Pending { yield_for }) => {
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
    pub fn try_poll_id(&self) -> Result<Poll<ID>> {
        Ok(Poll::Ready {
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

    fn next_id(&self, f: impl FnMut(ID::Ty)) -> ID {
        self.next_id(f)
    }

    fn try_next_id(&self, f: impl FnMut(ID::Ty)) -> Result<ID, Self::Err> {
        self.try_next_id(f)
    }

    fn poll_id(&self) -> Poll<ID> {
        self.poll_id()
    }

    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err> {
        self.try_poll_id()
    }
}
