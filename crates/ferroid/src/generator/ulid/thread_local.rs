//! Thread-local ULID generation utilities.
//!
//! Provides high-performance, non-monotonic and monotonic ULID generation using
//! thread-local generators.
//!
//! In rare cases where the generator saturates within the same millisecond
//! (monotonic overflow), it yields using the configured backoff strategy (e.g.,
//! spin, yield, sleep). These overflows typically resolve within ~1ms.
use crate::{
    BasicMonoUlidGenerator, BasicUlidGenerator, Id, IdGenStatus, MonotonicClock, RandSource,
    ThreadRandom, ToU64, ULID, UNIX_EPOCH,
};
use std::sync::LazyLock;
use std::thread_local;

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

/// Backoff strategies for handling monotonic ULID overflow.
///
/// If multiple ULIDs are generated in the same millisecond and the random
/// component is exhausted, the generator invokes one of these strategies to
/// wait before retrying.
#[derive(Debug, Clone, Copy)]
pub enum Backoff {
    /// Busy-waits in a tight loop.
    ///
    /// Offers maximum throughput at the cost of high CPU usage.
    Spin,

    /// Yields to the OS scheduler to allow other threads to run.
    ///
    /// More CPU-friendly than spinning, but may still busy-wait if no other
    /// threads are ready.
    Yield,

    /// Sleeps for the requested retry delay in milliseconds.
    ///
    /// Lowest CPU usage, but may oversleep depending on platform-specific
    /// scheduler resolution.
    Sleep,
}

pub struct Ulid;

impl Ulid {
    /// Generates a non-monotonic ULID that is always random, even when
    /// generated within the same millisecond.
    ///
    /// # Example
    /// ```
    /// #[cfg(all(feature = "ulid", feature = "thread_local"))]
    /// {
    ///     use ferroid::Ulid;
    ///     let id = Ulid::new_ulid();
    /// }
    /// ```
    #[must_use]
    pub fn new_ulid() -> ULID {
        BASIC_ULID.with(|g| match g.next_id() {
            IdGenStatus::Ready { id } => id,
            IdGenStatus::Pending { .. } => {
                unreachable!("A non-monotonic generator should never yield!")
            }
        })
    }

    /// Generates a monotonic ULID using [`Backoff::Yield`] as the overflow
    /// strategy.
    ///
    /// # Example
    /// ```
    /// #[cfg(all(feature = "ulid", feature = "thread_local"))]
    /// {
    ///     use ferroid::Ulid;
    ///     let id = Ulid::new_mono_ulid();
    /// }
    /// ```
    #[must_use]
    pub fn new_mono_ulid() -> ULID {
        Self::with_mono_backoff(Backoff::Yield)
    }

    /// Generates a monotonic ULID using the given [`Backoff`] strategy to
    /// handle overflow.
    ///
    /// If the generator exhausts available entropy within the same millisecond,
    /// the backoff strategy determines how it waits before retrying.
    ///
    /// # Example
    /// ```
    /// #[cfg(all(feature = "ulid", feature = "thread_local"))]
    /// {
    ///     use ferroid::{Ulid, Backoff};
    ///     let id = Ulid::with_mono_backoff(Backoff::Spin);
    /// }
    /// ```
    #[must_use]
    pub fn with_mono_backoff(strategy: Backoff) -> ULID {
        Self::ulid_mono(strategy)
    }

    /// Generates a ULID using the specified [`Backoff`] strategy.
    ///
    /// This is a convenient wrapper around [`ulid_mono_with_backoff`] with
    /// built-in strategies.
    fn ulid_mono(strategy: Backoff) -> ULID {
        Self::ulid_mono_with_backoff(|yield_for| match strategy {
            Backoff::Spin => core::hint::spin_loop(),
            Backoff::Yield => std::thread::yield_now(),
            Backoff::Sleep => {
                std::thread::sleep(core::time::Duration::from_millis(yield_for.to_u64()));
            }
        })
    }

    /// Generates a monotonic ULID using a custom backoff strategy.
    ///
    /// The provided function is called when the generator must wait before
    /// retrying due to ULID monotonic overflow. The `yield_for` argument
    /// indicates the recommended wait time in milliseconds.
    fn ulid_mono_with_backoff(f: impl Fn(<ULID as Id>::Ty)) -> ULID {
        BASIC_MONO_ULID.with(|g| {
            loop {
                match g.next_id() {
                    IdGenStatus::Ready { id } => break id,
                    IdGenStatus::Pending { yield_for } => f(yield_for),
                }
            }
        })
    }

    /// Creates a ULID from a given millisecond timestamp.
    ///
    /// Random bits are generated using the thread-local RNG. The resulting ID
    /// is not guaranteed to be monotonic.
    ///
    /// # Example
    /// ```
    /// #[cfg(all(feature = "ulid", feature = "thread_local"))]
    /// {
    ///     use ferroid::Ulid;
    ///     let id = Ulid::from_timestamp(1_694_201_234_000);
    /// }
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
    /// #[cfg(all(feature = "ulid", feature = "thread_local"))]
    /// {
    ///     use ferroid::{Ulid, ThreadRandom};
    ///     let id = Ulid::from_timestamp_and_rand(0, &ThreadRandom);
    /// }
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
    /// #[cfg(all(feature = "ulid", feature = "thread_local"))]
    /// {
    ///     use ferroid::Ulid;
    ///     let id = Ulid::from_datetime(std::time::SystemTime::now());
    /// }
    /// ```
    #[must_use]
    pub fn from_datetime(datetime: std::time::SystemTime) -> ULID {
        ULID::from_datetime(datetime)
    }

    /// Creates a ULID from a `SystemTime` and a custom RNG.
    ///
    /// # Example
    /// ```
    /// #[cfg(all(feature = "ulid", feature = "thread_local"))]
    /// {
    ///     use ferroid::{Ulid, ThreadRandom};
    ///     let now = std::time::SystemTime::now();
    ///     let id = Ulid::from_datetime_and_rand(now, &ThreadRandom);
    /// }
    /// ```
    pub fn from_datetime_and_rand<R>(datetime: std::time::SystemTime, rng: &R) -> ULID
    where
        R: RandSource<<ULID as Id>::Ty>,
    {
        ULID::from_datetime_and_rand(datetime, rng)
    }
}
