//! Thread-local ULID generation utilities.
//!
//! Provides high-performance, monotonic ULID generation using thread-local
//! generators.
//!
//! In rare cases where the generator saturates within the same millisecond
//! (monotonic overflow), it yields using the configured backoff strategy (e.g.,
//! spin, yield, sleep). These overflows typically resolve within ~1ms.
//!
//! # Example
//! ```rust
//! use ferroid::{ulid_mono, Backoff};
//!
//! let id = ulid_mono(Backoff::Yield);
//! println!("ULID: {}", id);
//! ```

use crate::{
    BasicUlidGenerator, Id, IdGenStatus, MonotonicClock, ThreadRandom, ToU64, ULID, UNIX_EPOCH,
};
use std::sync::LazyLock;

/// A global clock returning milliseconds since the Unix epoch, guaranteed to be
/// strictly monotonic.
static GLOBAL_MONOTONIC_CLOCK: LazyLock<MonotonicClock> =
    LazyLock::new(|| MonotonicClock::with_epoch(UNIX_EPOCH));

thread_local! {
    /// A thread-local ULID generator that reads from a global monotonic clock.
    static BASIC_MONO_ULID: BasicUlidGenerator<ULID, MonotonicClock, ThreadRandom> =
        BasicUlidGenerator::new(
            GLOBAL_MONOTONIC_CLOCK.clone(),
            ThreadRandom,
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

/// Generates a ULID using the specified [`Backoff`] strategy.
///
/// This is a convenient wrapper around [`ulid_mono_with_backoff`] with built-in
/// strategies.
///
/// # Example
/// ```rust
/// use ferroid::{ulid_mono, Backoff};
///
/// let id = ulid_mono(Backoff::Yield);
/// ```
pub fn ulid_mono(strategy: Backoff) -> ULID {
    ulid_mono_with_backoff(|yield_for| match strategy {
        Backoff::Spin => core::hint::spin_loop(),
        Backoff::Yield => std::thread::yield_now(),
        Backoff::Sleep => {
            std::thread::sleep(core::time::Duration::from_millis(
                yield_for
                    .to_u64()
                    .expect("ULID timestamp should always fit in u64 (48 bits)"),
            ));
        }
    })
}

/// Generates a ULID using a custom backoff strategy.
///
/// The provided function is called when the generator must wait before retrying
/// due to ULID monotonic overflow. The `yield_for` argument indicates the
/// recommended wait time in milliseconds.
///
/// # Example
/// ```rust
/// use ferroid::{ulid_mono_with_backoff, ToU64};
///
/// let id = ulid_mono_with_backoff(|yield_for| {
///     let delay = yield_for
///         .to_u64()
///         .expect("ULID timestamp should always fit in u64 (48 bits)") * 2;
///     std::thread::sleep(std::time::Duration::from_millis(delay));
/// });
/// ```
pub fn ulid_mono_with_backoff(f: impl Fn(<ULID as Id>::Ty)) -> ULID {
    BASIC_MONO_ULID.with(|g| {
        loop {
            match g.next_id() {
                IdGenStatus::Ready { id } => break id,
                IdGenStatus::Pending { yield_for } => f(yield_for),
            }
        }
    })
}
