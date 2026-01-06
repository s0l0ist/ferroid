//! Thread-local ULID generation utilities.
//!
//! Provides high-performance, non-monotonic and monotonic ULID generation using
//! thread-local generators.
//!
//! In rare cases where the generator saturates within the same millisecond
//! (monotonic overflow), it yields using the configured backoff strategy (e.g.,
//! spin, yield, sleep). These overflows typically resolve within ~1ms.
use std::{sync::LazyLock, thread_local};

use crate::{
    generator::{BasicMonoUlidGenerator, BasicUlidGenerator, Poll},
    id::{Id, ULID},
    rand::{RandSource, ThreadRandom},
    time::{MonotonicClock, UNIX_EPOCH},
};

/// A global clock returning milliseconds since the Unix epoch, guaranteed to be
/// strictly monotonic.
static GLOBAL_MONOTONIC_CLOCK: LazyLock<MonotonicClock> =
    LazyLock::new(|| MonotonicClock::with_epoch(UNIX_EPOCH));

thread_local! {
    /// A thread-local, non-monotonic ULID generator that reads from a global
    /// monotonic clock.
    static BASIC_ULID: BasicUlidGenerator<ULID, MonotonicClock, ThreadRandom> =
        BasicUlidGenerator::new(
            GLOBAL_MONOTONIC_CLOCK.clone(),
            ThreadRandom
        );

    /// A thread-local, monotonic ULID generator that reads from a global
    /// monotonic clock.
    static BASIC_MONO_ULID: BasicMonoUlidGenerator<ULID, MonotonicClock, ThreadRandom> =
        BasicMonoUlidGenerator::new(
            GLOBAL_MONOTONIC_CLOCK.clone(),
            ThreadRandom
        );
}

/// A thread-local ULID generator with monotonic and non-monotonic modes.
///
/// Provides fast, per-thread ULID generation using a shared monotonic clock and
/// thread-local RNG. Monotonic overflows are handled with configurable
/// [`Backoff`] strategies.
pub struct Ulid;

impl Ulid {
    /// Generates a new **non-monotonic** ULID using the thread-local generator.
    ///
    /// Always samples fresh randomness (even within the same millisecond), so
    /// IDs are not strictly increasing when timestamps are equal.
    ///
    /// **Collision note:** with multiple independent generators (e.g. `N > 1`
    /// threads), purely-random generation has a higher collision probability
    /// than monotonic generation. If monotonic IDs are acceptable for your use
    /// case, prefer [`Self::new_ulid_mono`] to further reduce collision risk.
    ///
    /// See: [Collision Probability Analysis] (README).
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::Ulid;
    /// let id = Ulid::new_ulid();
    /// ```
    ///
    /// [Collision Probability Analysis]:
    ///     https://github.com/s0l0ist/ferroid/tree/main/crates/ferroid#collision-probability-analysis
    #[must_use]
    pub fn new_ulid() -> ULID {
        BASIC_ULID.with(|g| match g.poll_id() {
            Poll::Ready { id } => id,
            Poll::Pending { .. } => unreachable!("basic ULID generator should never need to yield"),
        })
    }

    /// Generates a new **monotonic** ULID using the thread-local generator.
    ///
    /// Within a given millisecond, IDs are strictly increasing **per thread**
    /// by incrementing the random component. Across threads, sequences are
    /// independent (no global ordering).
    ///
    /// Compared to [`Self::new_ulid`], this typically reduces collision
    /// probability when multiple generators are active (e.g. `N > 1` threads).
    /// See: [Collision Probability Analysis] (README).
    ///
    /// If the random space saturates for the current millisecond, the generator
    /// retries using the provided callback.
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::Ulid;
    /// let id = Ulid::new_ulid_mono(|_| std::thread::yield_now());
    /// ```
    ///
    /// [Collision Probability Analysis]:
    ///     https://github.com/s0l0ist/ferroid/tree/main/crates/ferroid#collision-probability-analysis
    #[must_use]
    pub fn new_ulid_mono(f: impl FnMut(<ULID as Id>::Ty)) -> ULID {
        BASIC_MONO_ULID.with(|g| g.next_id(f))
    }

    /// Creates a ULID from a given millisecond timestamp.
    ///
    /// Random bits are generated using the thread-local RNG. The resulting ID
    /// is not guaranteed to be monotonic.
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::Ulid;
    /// let id = Ulid::from_timestamp(1_694_201_234_000);
    /// ```
    #[must_use]
    pub fn from_timestamp(timestamp: <ULID as Id>::Ty) -> ULID {
        ULID::from_timestamp(timestamp)
    }

    /// Creates a ULID from a given millisecond timestamp and a custom RNG.
    ///
    /// Useful in deterministic or testable scenarios where a specific random
    /// source is preferred.
    ///
    /// # Example
    /// ```
    /// use ferroid::{generator::thread_local::Ulid, rand::ThreadRandom};
    /// let id = Ulid::from_timestamp_and_rand(0, &ThreadRandom);
    /// ```
    pub fn from_timestamp_and_rand<R>(timestamp: <ULID as Id>::Ty, rng: &R) -> ULID
    where
        R: RandSource<<ULID as Id>::Ty>,
    {
        ULID::from_timestamp_and_rand(timestamp, rng)
    }

    /// Creates a ULID from a `SystemTime`.
    ///
    /// This is a convenience wrapper over [`Ulid::from_timestamp`] that
    /// extracts the Unix timestamp in milliseconds.
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::Ulid;
    /// let id = Ulid::from_datetime(std::time::SystemTime::now());
    /// ```
    #[must_use]
    pub fn from_datetime(datetime: std::time::SystemTime) -> ULID {
        ULID::from_datetime(datetime)
    }

    /// Creates a ULID from a `SystemTime` and a custom RNG.
    ///
    /// # Example
    /// ```
    /// use ferroid::{generator::thread_local::Ulid, rand::ThreadRandom};
    /// let now = std::time::SystemTime::now();
    /// let id = Ulid::from_datetime_and_rand(now, &ThreadRandom);
    /// ```
    pub fn from_datetime_and_rand<R>(datetime: std::time::SystemTime, rng: &R) -> ULID
    where
        R: RandSource<<ULID as Id>::Ty>,
    {
        ULID::from_datetime_and_rand(datetime, rng)
    }
}
