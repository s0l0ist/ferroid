use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Custom epoch: Wednesday, January 1, 2025 00:00:00 UTC
pub const CUSTOM_EPOCH: Duration = Duration::from_millis(1_735_689_600_000);

/// Twitter epoch: Thursday, November 4, 2010 1:42:54.657 UTC
pub const TWITTER_EPOCH: Duration = Duration::from_millis(1_288_834_974_657);

/// Discord epoch: Thursday, January 1, 2015 00:00:00 UTC
pub const DISCORD_EPOCH: Duration = Duration::from_millis(1_420_070_400_000);

/// Instagram epoch: Saturday, January 1, 2011 00:00:00 UTC
pub const INSTAGRAM_EPOCH: Duration = Duration::from_millis(1_293_840_000_000);

/// Mastodon uses standard UNIX epoch: Thursday, January 1, 1970 00:00:00 UTC
pub const MASTODON_EPOCH: Duration = Duration::from_millis(0);

/// A trait for time sources that return a monotonic or wall-clock timestamp.
///
/// This abstraction allows you to plug in a real system clock, a monotonic
/// timer, or a mocked time source in tests.
///
/// The timestamp type `T` is generic (typically `u64` or `u128`), and the unit
/// is expected to be **milliseconds** relative to a configurable origin.
///
/// # Example
///
/// ```
/// use ferroid::TimeSource;
///
/// struct FixedTime;
/// impl TimeSource<u64> for FixedTime {
///     fn current_millis(&self) -> u64 {
///         1234
///     }
/// }
///
/// let time = FixedTime;
/// assert_eq!(time.current_millis(), 1234);
/// ```
pub trait TimeSource<T> {
    /// Returns the current time in milliseconds since the configured epoch.
    fn current_millis(&self) -> T;
}

/// Shared ticker thread that updates every millisecond.
struct SharedTickerInner {
    current: AtomicU64,
    _handle: OnceLock<JoinHandle<()>>,
}

/// A monotonic time source that returns elapsed time since process start,
/// offset from a user-defined epoch.
///
/// This avoids wall-clock adjustments (e.g., NTP or daylight savings changes)
/// while still aligning timestamps to a fixed origin.
///
/// Internally, the clock measures time by capturing `Instant::now()` at
/// construction and adding to it the duration elapsed since a given epoch
/// (computed from `SystemTime::now()` at startup).
#[derive(Clone)]
pub struct MonotonicClock {
    inner: Arc<SharedTickerInner>,
    epoch_offset: u64, // in milliseconds
}

impl Default for MonotonicClock {
    /// Constructs a monotonic clock aligned to the default [`CUSTOM_EPOCH`].
    ///
    /// Panics if system time is earlier than the custom epoch.
    fn default() -> Self {
        Self::with_epoch(CUSTOM_EPOCH)
    }
}

impl MonotonicClock {
    /// Constructs a monotonic clock using a custom epoch as the origin (t = 0),
    /// specified in milliseconds since the Unix epoch.
    ///
    /// The provided epoch defines the zero-point for all future timestamps
    /// returned by this clock. Internally, the clock spawns a background thread
    /// that updates a shared atomic counter once per millisecond, using a
    /// monotonic timer (`Instant`) to measure elapsed time since startup.
    ///
    /// On each call to [`current_millis`], the clock returns the current tick
    /// value plus a fixed offset — the precomputed difference between the
    /// current wall-clock time (`SystemTime::now()`) and the given epoch.
    ///
    /// This design avoids syscalls on the hot path and ensures that time never
    /// goes backward, even if the system clock is adjusted externally.
    ///
    /// # Parameters
    ///
    /// - `epoch`: The origin timestamp, as a [`Duration`] since 1970-01-01 UTC.
    ///
    /// # Panics
    ///
    /// Panics if:
    ///
    /// - The current system time is earlier than the given epoch
    /// - The internal ticker thread has already been initialized
    ///
    /// # Example
    ///
    /// ```
    /// use std::time::Instant;
    /// use ferroid::{MonotonicClock, TimeSource};
    /// let now = std::time::SystemTime::now()
    ///     .duration_since(std::time::UNIX_EPOCH)
    ///     .unwrap();
    ///
    /// // Or use a default epoch
    /// // use ferroid::TWITTER_EPOCH,
    /// // let now = TWITTER_EPOCH;
    /// let start = Instant::now();
    /// let clock = MonotonicClock::with_epoch(now);
    /// std::thread::sleep(std::time::Duration::from_millis(5));
    ///
    /// let elapsed_us = start.elapsed().as_micros();
    /// let ts = clock.current_millis();
    ///
    /// panic!("GOT TS: {:?}, elapsed_us {}", ts, elapsed_us);
    /// ```
    ///
    /// This allows you to control the timestamp layout (e.g., Snowflake-style
    /// ID encoding) by anchoring all generated times to a custom epoch of your
    /// choosing.
    pub fn with_epoch(epoch: Duration) -> Self {
        let system_now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System clock before UNIX_EPOCH");
        let offset = system_now
            .checked_sub(epoch)
            .expect("System clock before custom epoch")
            .as_millis() as u64;

        let inner = Arc::new(SharedTickerInner {
            current: AtomicU64::new(0),
            _handle: OnceLock::new(),
        });

        let weak_inner = Arc::downgrade(&inner);
        let handle = thread::spawn(move || {
            let start = Instant::now();
            let mut tick = 0;

            loop {
                let Some(inner_ref) = weak_inner.upgrade() else {
                    break;
                };

                // Compute the absolute target time of the next tick
                let target = start + Duration::from_millis(tick);

                // Sleep if we are early
                let now = Instant::now();
                if now < target {
                    thread::sleep(target - now);
                }

                // After waking, recompute how far we actually are from the
                // start
                let now_ms = start.elapsed().as_millis() as u64;

                // Monotonic store, aligned to elapsed milliseconds since start
                inner_ref.current.store(now_ms, Ordering::Relaxed);

                // Align to next tick after the current actual time
                tick = now_ms + 1;
            }
        });

        let _ = inner
            ._handle
            .set(handle)
            .expect("failed to set thread handle");

        Self {
            inner,
            epoch_offset: offset,
        }
    }
}

impl TimeSource<u64> for MonotonicClock {
    /// Returns the number of milliseconds since the configured epoch, based on
    /// the elapsed monotonic time since construction.
    fn current_millis(&self) -> u64 {
        self.epoch_offset + self.inner.current.load(Ordering::Acquire)
    }
}

// I'm including this test to check if the drop causes the thread to terminate.
// Run this test manually by inserting a println into the inner loop
// #[cfg(test)] mod tests { use super::*;
//
//     #[test]
//     fn test_monotonic_clock_drop_terminates_thread() {
//         use std::time::Duration;
//
//         let clock = MonotonicClock::with_epoch(CUSTOM_EPOCH);
//         std::thread::sleep(Duration::from_millis(1));
//
//         drop(clock);
//     }
// }
