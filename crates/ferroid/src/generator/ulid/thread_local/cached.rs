//! Thread-local ULID generation using [`CachedClock`].
//!
//! Requires thread support and pointer-width and 64-bit atomics.
//!
//! [`CachedClock`]: crate::time::CachedClock
use core::time::Duration;
use std::thread_local;

use crate::{
    generator::{BasicMonoUlidGenerator, BasicUlidGenerator, Poll},
    id::{Id, ULID},
    rand::{RandSource, ThreadRandom},
    time::CachedClock,
};

thread_local! {
    /// A thread-local, non-monotonic ULID generator that reads from a global
    /// wall-clock cache.
    static BASIC_ULID: BasicUlidGenerator<ULID, CachedClock, ThreadRandom> =
        BasicUlidGenerator::new(
            CachedClock::default(),
            ThreadRandom
        );

    /// A thread-local, monotonic ULID generator that reads from a global
    /// wall-clock cache.
    static BASIC_MONO_ULID: BasicMonoUlidGenerator<ULID, CachedClock, ThreadRandom> =
        BasicMonoUlidGenerator::new(
            CachedClock::default(),
            ThreadRandom
        );
}

/// An opt-in thread-local ULID generator using a shared wall-clock cache.
///
/// Prefer [`direct::Ulid`](super::direct::Ulid) for general-purpose use.
/// Consider this variant when profiling a high-throughput workload shows that
/// per-call clock reads are a bottleneck.
///
/// The first clock-based generation starts the shared [`CachedClock`]
/// background thread. It refreshes the timestamp roughly once per millisecond
/// and runs for the lifetime of the process. Thread scheduling can delay
/// refreshes. Constructors that accept a timestamp or `SystemTime` do not
/// start this thread.
///
/// Both monotonic and non-monotonic generation are supported, with the same
/// API as `direct::Ulid`. This variant requires thread support; use
/// `direct::Ulid` on `wasm32-unknown-unknown`.
///
/// [`CachedClock`]: crate::time::CachedClock
pub struct Ulid;

impl Ulid {
    /// Generates a new **non-monotonic** ULID using the thread-local generator.
    ///
    /// Always samples fresh randomness (even within the same millisecond), so
    /// IDs are not strictly increasing when timestamps are equal.
    ///
    /// Repeated random draws can collide even within one thread. For strictly
    /// increasing IDs within one thread, use [`Self::new_ulid_mono`].
    ///
    /// See the [collision probability analysis](https://github.com/s0l0ist/ferroid/blob/main/crates/ferroid/docs/collision-analysis.md).
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::cached;
    /// let id = cached::Ulid::new_ulid();
    /// ```
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
    /// by incrementing the random component. Across threads, monotonic streams
    /// are independent (no global ordering).
    ///
    /// Compared to [`Self::new_ulid`], this typically reduces collision
    /// probability when multiple generators are active (e.g. `N > 1` threads).
    /// See the [collision probability analysis](https://github.com/s0l0ist/ferroid/blob/main/crates/ferroid/docs/collision-analysis.md).
    ///
    /// If the random space saturates or wall time moves backwards, the generator
    /// retries using the provided callback. The callback receives a suggested
    /// [`Duration`], including [`Duration::ZERO`].
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::cached;
    /// let id = cached::Ulid::new_ulid_mono(|_| std::thread::yield_now());
    /// ```
    #[must_use]
    pub fn new_ulid_mono(f: impl FnMut(Duration)) -> ULID {
        BASIC_MONO_ULID.with(|g| g.next_id(f))
    }

    /// Creates a ULID from a given millisecond timestamp.
    ///
    /// Random bits are generated using the thread-local RNG. The resulting ID
    /// is not guaranteed to be monotonic.
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::cached;
    /// let id = cached::Ulid::from_timestamp(1_694_201_234_000);
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
    /// use ferroid::{generator::thread_local::cached, rand::ThreadRandom};
    /// let id = cached::Ulid::from_timestamp_and_rand(0, &ThreadRandom);
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
    /// use ferroid::generator::thread_local::cached;
    /// let id = cached::Ulid::from_system_time(std::time::SystemTime::now());
    /// ```
    #[must_use]
    pub fn from_system_time(system_time: std::time::SystemTime) -> ULID {
        ULID::from_system_time(system_time)
    }

    /// Creates a ULID from a `SystemTime` and a custom RNG.
    ///
    /// # Example
    /// ```
    /// use ferroid::{generator::thread_local::cached, rand::ThreadRandom};
    /// let now = std::time::SystemTime::now();
    /// let id = cached::Ulid::from_system_time_and_rand(now, &ThreadRandom);
    /// ```
    pub fn from_system_time_and_rand<R>(system_time: std::time::SystemTime, rng: &R) -> ULID
    where
        R: RandSource<<ULID as Id>::Ty>,
    {
        ULID::from_system_time_and_rand(system_time, rng)
    }
}

#[cfg(test)]
mod tests {
    use super::Ulid;

    #[test]
    fn monotonic_generation_accepts_sleep_backoff() {
        let first = Ulid::new_ulid_mono(std::thread::sleep);
        let second = Ulid::new_ulid_mono(std::thread::sleep);
        assert!(second > first);
    }
}
