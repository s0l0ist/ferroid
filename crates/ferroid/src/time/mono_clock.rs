use alloc::sync::Arc;
use core::time::Duration;
use std::{
    sync::{LazyLock, OnceLock},
    thread::{self, JoinHandle},
    time::{Instant, SystemTime},
};

use portable_atomic::{AtomicU64, Ordering};

use crate::time::{TimeSource, UNIX_EPOCH};

/// Lazily initialized, process-global ticker.
///
/// The background thread is started on first use of [`MonotonicClock`], and
/// it runs for the lifetime of the process.
static GLOBAL_TICKER: LazyLock<Arc<SharedTickerInner>> = LazyLock::new(|| {
    let start = Instant::now();
    let system_now = SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    #[allow(clippy::cast_possible_truncation)]
    let base_system_now = system_now.as_millis() as u64;

    let inner = Arc::new(SharedTickerInner {
        current: AtomicU64::new(0),
        handle: OnceLock::new(),
        base_system_now,
    });

    let inner_ref = Arc::clone(&inner);
    let handle = thread::spawn(move || {
        let mut tick = 0;

        loop {
            // Compute the absolute target time of the next tick
            let target = start + Duration::from_millis(tick);

            // Sleep if we are early
            let now = Instant::now();
            if now < target {
                thread::sleep(target - now);
            }

            // After waking, recompute how far we actually are from the
            // start
            #[allow(clippy::cast_possible_truncation)]
            let now_ms = start.elapsed().as_millis() as u64;

            // Monotonic store, aligned to elapsed milliseconds since start
            inner_ref.current.store(now_ms, Ordering::Relaxed);

            // Align to next tick after the current actual time
            tick = now_ms + 1;
        }
    });

    let _ = inner.handle.set(handle);
    inner
});

/// Shared ticker thread that updates every millisecond.
#[derive(Debug)]
struct SharedTickerInner {
    current: AtomicU64,
    handle: OnceLock<JoinHandle<()>>,
    base_system_now: u64,
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
///
/// `N` controls the number of real milliseconds represented by one returned
/// time unit. `MonotonicClock` and `MonotonicClock<1>` return literal
/// milliseconds, while `MonotonicClock<8>` returns 8-millisecond ticks.
///
/// ```compile_fail
/// use ferroid::time::{MonotonicClock, UNIX_EPOCH};
///
/// let _ = MonotonicClock::<0>::with_epoch(UNIX_EPOCH);
/// ```
#[derive(Clone, Debug)]
pub struct MonotonicClock<const N: u64 = 1> {
    inner: Arc<SharedTickerInner>,
    epoch_offset: u64, // in milliseconds
}

impl Default for MonotonicClock<1> {
    /// Constructs a monotonic clock aligned to the default [`UNIX_EPOCH`].
    fn default() -> Self {
        Self::with_epoch(UNIX_EPOCH)
    }
}

impl<const N: u64> MonotonicClock<N> {
    const ASSERT_VALID_GRANULARITY: () = assert!(
        N > 0,
        "MonotonicClock granularity must be greater than zero"
    );
    pub const GRANULARITY_MILLIS: u64 = N;

    /// Constructs a monotonic clock using a custom epoch as the origin (t = 0),
    /// specified in milliseconds since the Unix epoch.
    ///
    /// The provided epoch defines the zero-point for all future timestamps
    /// returned by this clock. Internally, the clock uses a shared background
    /// thread that updates a global atomic counter once per millisecond, using
    /// a monotonic timer (`Instant`) to measure elapsed time since startup.
    ///
    /// Different epochs are supported by applying a per-instance offset to the
    /// shared ticker value.
    ///
    /// On each call to [`current_millis`], the clock returns the current tick
    /// value plus a fixed offset - the precomputed difference between the
    /// current wall-clock time (`SystemTime::now()`) and the given epoch. The
    /// final value is quantized into `N`-millisecond units.
    ///
    /// This design avoids syscalls on the hot path and ensures that time never
    /// goes backward, even if the system clock is adjusted externally.
    ///
    /// # Parameters
    ///
    /// - `epoch`: The origin timestamp, as a [`Duration`] since 1970-01-01 UTC.
    ///
    /// # Example
    /// ```
    /// use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
    ///
    /// use ferroid::time::{MonotonicClock, TimeSource};
    /// let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    /// // Or use a default epoch
    /// // use ferroid::TWITTER_EPOCH,
    /// // let now = TWITTER_EPOCH;
    ///
    /// let clock = MonotonicClock::<1>::with_epoch(now);
    ///
    /// std::thread::sleep(Duration::from_millis(5));
    ///
    /// let ts: u64 = clock.current_millis();
    ///
    /// // On most systems, this will report a value near 5 ms. However, due to
    /// // small differences in timer alignment and sleep accuracy, the counter
    /// // may be slightly behind or ahead on the first read. It's common to
    /// // observe values like 4–6 ms after sleeping for 5 ms. The value will
    /// // never go backward and is guaranteed to increase monotonically.
    /// // assert!(ts >= ~5);
    /// ```
    ///
    /// This allows you to control the timestamp layout by anchoring all
    /// generated times to a custom epoch of your choosing.
    ///
    /// [`current_millis`]: TimeSource::current_millis
    #[must_use]
    pub fn with_epoch(epoch: Duration) -> Self {
        let () = Self::ASSERT_VALID_GRANULARITY;
        let inner = Arc::clone(&GLOBAL_TICKER);

        #[allow(clippy::cast_possible_truncation)]
        let offset = inner
            .base_system_now
            .saturating_sub(epoch.as_millis() as u64);

        Self {
            inner,
            epoch_offset: offset,
        }
    }
}

impl<const N: u64> TimeSource<u64> for MonotonicClock<N> {
    const GRANULARITY_MILLIS: u64 = Self::GRANULARITY_MILLIS;

    /// Returns the number of `N`-millisecond units since the configured epoch,
    /// based on the elapsed monotonic time since construction.
    fn current_millis(&self) -> u64 {
        let () = Self::ASSERT_VALID_GRANULARITY;
        (self.epoch_offset + self.inner.current.load(Ordering::Relaxed)) / N
    }
}

impl<const N: u64> TimeSource<u128> for MonotonicClock<N> {
    const GRANULARITY_MILLIS: u64 = Self::GRANULARITY_MILLIS;

    /// Returns the number of `N`-millisecond units since the configured epoch,
    /// based on the elapsed monotonic time since construction.
    fn current_millis(&self) -> u128 {
        u128::from(<Self as TimeSource<u64>>::current_millis(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_granularity_stays_in_milliseconds() {
        let clock = MonotonicClock::default();
        let _ts: u64 = <MonotonicClock as TimeSource<u64>>::current_millis(&clock);
        assert_eq!(MonotonicClock::<1>::GRANULARITY_MILLIS, 1);
        assert_eq!(<MonotonicClock as TimeSource<u64>>::GRANULARITY_MILLIS, 1);
    }

    #[test]
    fn quantized_granularity_scales_current_millis() {
        let millis_clock = MonotonicClock::<1>::with_epoch(UNIX_EPOCH);
        let quantized_clock = MonotonicClock::<8>::with_epoch(UNIX_EPOCH);

        let lower = <MonotonicClock<1> as TimeSource<u64>>::current_millis(&millis_clock);
        let quantized = <MonotonicClock<8> as TimeSource<u64>>::current_millis(&quantized_clock);
        let upper = <MonotonicClock<1> as TimeSource<u64>>::current_millis(&millis_clock);

        assert_eq!(MonotonicClock::<8>::GRANULARITY_MILLIS, 8);
        assert_eq!(
            <MonotonicClock<8> as TimeSource<u64>>::GRANULARITY_MILLIS,
            8
        );
        assert!(lower / 8 <= quantized);
        assert!(quantized <= upper / 8);
    }
}
