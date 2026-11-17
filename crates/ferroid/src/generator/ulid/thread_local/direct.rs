//! Thread-local ULID generation using [`DirectClock`].
//!
//! [`DirectClock`]: crate::time::DirectClock
use core::time::Duration;
use std::thread_local;

use crate::{
    generator::BasicMonoUlidGenerator,
    id::{Id, ULID},
    rand::{RandSource, ThreadRandom},
    time::DirectClock,
};

thread_local! {
    /// A thread-local, monotonic ULID generator using [`DirectClock`].
    static BASIC_MONO_ULID_DIRECT: BasicMonoUlidGenerator<ULID, DirectClock, ThreadRandom> =
        BasicMonoUlidGenerator::new(
            DirectClock::default(),
            ThreadRandom
        );
}

/// A thread-local ULID generator for general-purpose use.
///
/// Reads current wall time on every call and starts no
/// background thread. Supports both non-monotonic and monotonic generation.
/// It can also be used on `wasm32-unknown-unknown` with the required entropy
/// backend enabled; see the crate's platform support
/// documentation.
///
/// For high-throughput workloads where profiling shows that clock reads are
/// a bottleneck, [`cached::Ulid`] offers the same API using a background clock
/// cache. Constructors that accept a timestamp or `SystemTime` do not read
/// either clock.
///
/// # Example
/// ```
/// use ferroid::generator::thread_local::direct;
///
/// let id = direct::Ulid::new_ulid();
/// let mono = direct::Ulid::new_ulid_mono(|_| std::thread::yield_now());
/// ```
///
/// [`cached::Ulid`]: super::cached::Ulid
/// [`DirectClock`]: crate::time::DirectClock
pub struct Ulid;

impl Ulid {
    /// Generates a new **non-monotonic** ULID using the current wall time and
    /// fresh randomness from the thread-local RNG.
    ///
    /// Equivalent to [`ULID::now()`], without accessing a thread-local generator.
    ///
    /// IDs generated within the same millisecond have no guaranteed ordering
    /// and can collide. For strictly increasing IDs within one thread, use
    /// [`Self::new_ulid_mono`].
    ///
    /// See the [collision probability analysis](https://github.com/s0l0ist/ferroid/blob/main/crates/ferroid/docs/collision-analysis.md).
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::direct;
    /// let id = direct::Ulid::new_ulid();
    /// ```
    ///
    /// [`DirectClock`]: crate::time::DirectClock
    #[must_use]
    pub fn new_ulid() -> ULID {
        ULID::now()
    }

    /// Generates a new **monotonic** ULID using the thread-local
    /// [`DirectClock`]-backed generator.
    ///
    /// IDs are strictly increasing **per thread** by incrementing the random
    /// component when the timestamp is unchanged. Threads have independent
    /// generators, so this does not establish ordering across threads.
    ///
    /// The backoff callback receives a suggested [`Duration`] when
    /// the random space is exhausted or wall time moves backwards. Generation
    /// waits until wall time catches up.
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::direct;
    /// let id = direct::Ulid::new_ulid_mono(|_| std::thread::yield_now());
    /// ```
    ///
    /// [`DirectClock`]: crate::time::DirectClock
    #[must_use]
    pub fn new_ulid_mono(f: impl FnMut(Duration)) -> ULID {
        BASIC_MONO_ULID_DIRECT.with(|g| g.next_id(f))
    }

    /// Creates a ULID from a given millisecond timestamp.
    ///
    /// Random bits are generated using the thread-local RNG. The resulting ID
    /// is not guaranteed to be monotonic. This does not read a clock.
    ///
    /// # Example
    /// ```
    /// use ferroid::generator::thread_local::direct;
    /// let id = direct::Ulid::from_timestamp(1_694_201_234_000);
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
    /// use ferroid::{generator::thread_local::direct, rand::ThreadRandom};
    /// let id = direct::Ulid::from_timestamp_and_rand(0, &ThreadRandom);
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
    /// use ferroid::generator::thread_local::direct;
    /// let id = direct::Ulid::from_system_time(std::time::SystemTime::now());
    /// ```
    #[must_use]
    pub fn from_system_time(system_time: std::time::SystemTime) -> ULID {
        ULID::from_system_time(system_time)
    }

    /// Creates a ULID from a `SystemTime` and a custom RNG.
    ///
    /// # Example
    /// ```
    /// use ferroid::{generator::thread_local::direct, rand::ThreadRandom};
    /// let now = std::time::SystemTime::now();
    /// let id = direct::Ulid::from_system_time_and_rand(now, &ThreadRandom);
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
