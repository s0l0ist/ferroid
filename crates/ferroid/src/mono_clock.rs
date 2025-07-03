use crate::{CUSTOM_EPOCH, TimeSource};
use core::time::Duration;
use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
/// Shared ticker thread that updates every millisecond.
#[derive(Debug)]
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
#[derive(Clone, Debug)]
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
    /// value plus a fixed offset - the precomputed difference between the
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
    /// use std::time::{Duration, Instant};
    /// use ferroid::{MonotonicClock, TimeSource};
    /// let now = std::time::SystemTime::now()
    ///     .duration_since(std::time::UNIX_EPOCH)
    ///     .unwrap();
    ///
    /// // Or use a default epoch
    /// // use ferroid::TWITTER_EPOCH,
    /// // let now = TWITTER_EPOCH;
    /// let clock = MonotonicClock::with_epoch(now);
    ///
    /// std::thread::sleep(Duration::from_millis(5));
    ///
    /// let ts: u64 = clock.current_millis();
    ///
    /// // On most systems, this will report a value near 5 ms. However, due to
    /// // small differences in timer alignment and sleep accuracy, the counter
    /// // may be slightly behind. It's common to observe values like 4–6 ms
    /// // after sleeping for 5 ms. The value will never go backward and is
    /// // guaranteed to increase monotonically.
    /// // assert!(ts >= 5);
    /// ```
    ///
    /// This allows you to control the timestamp layout (e.g., Snowflake-style
    /// ID encoding) by anchoring all generated times to a custom epoch of your
    /// choosing.
    ///
    /// [`current_millis`]: TimeSource::current_millis
    pub fn with_epoch(epoch: Duration) -> Self {
        let start = Instant::now();
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

        inner
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
        self.epoch_offset + self.inner.current.load(Ordering::Relaxed)
    }
}

impl TimeSource<u128> for MonotonicClock {
    /// Returns the number of milliseconds since the configured epoch, based on
    /// the elapsed monotonic time since construction.
    fn current_millis(&self) -> u128 {
        <Self as TimeSource<u64>>::current_millis(self) as u128
    }
}
