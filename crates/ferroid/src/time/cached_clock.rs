use core::{num::NonZeroU64, time::Duration};
use std::{
    sync::{
        Arc, LazyLock,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::Instant,
};

use crate::time::{DirectClock, TimeSource, UNIX_EPOCH};

/// A process-global cache of wall-clock milliseconds since the Unix epoch.
///
/// The thread starts on first use and runs for the lifetime of the process.
static CACHED_UNIX_MILLIS: LazyLock<Arc<AtomicU64>> = LazyLock::new(|| {
    let start = Instant::now();
    let clock = DirectClock::default();
    let current = Arc::new(AtomicU64::new(clock.current_millis()));
    let shared = Arc::clone(&current);
    let _ = thread::spawn(move || {
        let mut tick = 0;

        loop {
            // Anchor deadlines to the start so refresh work and late wakeups
            // do not accumulate scheduling drift.
            let Some(target) = start.checked_add(Duration::from_millis(tick)) else {
                return;
            };
            let now = Instant::now();
            if now < target {
                thread::sleep(target.duration_since(now));
            }

            shared.store(clock.current_millis(), Ordering::Relaxed);

            // Skip missed ticks and align to the next actual millisecond.
            let Some(next_tick) = u64::try_from(start.elapsed().as_millis())
                .ok()
                .and_then(|millis| millis.checked_add(1))
            else {
                return;
            };
            tick = next_tick;
        }
    });
    current
});

/// A wall-clock time source refreshed by a shared background thread roughly
/// once per millisecond.
///
/// Reads load the latest cached timestamp.
///
/// Refresh deadlines are anchored to a monotonic timer. Late wakeups skip
/// missed ticks and resume on the next millisecond boundary, keeping refresh
/// work and sleep overshoot from accumulating into scheduling drift.
///
/// The cache follows wall-clock corrections, including backwards jumps.
/// Snowflake and monotonic ULID generators return [`Poll::Pending`] while the
/// clock is behind their last generated timestamp. Thread scheduling can delay
/// cache updates; callers requiring a fresh wall-clock read can use
/// [`DirectClock`].
/// Use `DirectClock` on targets without thread support, including
/// `wasm32-unknown-unknown`.
///
/// `N` sets the number of real milliseconds represented by one returned unit.
/// `CachedClock<1>` returns milliseconds; `CachedClock<8>` returns 8 ms units.
/// It does not change the cache's refresh interval.
///
/// # Granularity
///
/// A zero granularity is rejected at compile time.
///
/// ```compile_fail
/// use ferroid::time::{CachedClock, UNIX_EPOCH};
/// let _ = CachedClock::<0>::with_epoch(UNIX_EPOCH);
/// ```
///
/// # Example
/// ```
/// use ferroid::time::{CachedClock, TWITTER_EPOCH, TimeSource};
/// let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
/// let _ms: u64 = clock.current_millis();
/// ```
///
/// [`Poll::Pending`]: crate::generator::Poll::Pending
#[derive(Clone, Debug)]
pub struct CachedClock<const N: u64 = 1> {
    current: Arc<AtomicU64>,
    epoch_millis: u64,
}

impl Default for CachedClock<1> {
    fn default() -> Self {
        Self::with_epoch(UNIX_EPOCH)
    }
}

impl<const N: u64> CachedClock<N> {
    /// The number of real milliseconds represented by one returned unit.
    pub const GRANULARITY_MILLIS: NonZeroU64 =
        NonZeroU64::new(N).expect("clock granularity must be greater than zero");

    /// Sets the timestamp origin as a duration since the Unix epoch.
    ///
    /// Times before this origin return zero. Sub-millisecond epoch precision is
    /// discarded, and millisecond values exceeding `u64::MAX` saturate.
    #[must_use]
    pub fn with_epoch(epoch: Duration) -> Self {
        let _ = Self::GRANULARITY_MILLIS;
        Self {
            current: Arc::clone(&CACHED_UNIX_MILLIS),
            epoch_millis: u64::try_from(epoch.as_millis()).unwrap_or(u64::MAX),
        }
    }
}

impl<const N: u64> TimeSource<u64> for CachedClock<N> {
    const GRANULARITY_MILLIS: NonZeroU64 = Self::GRANULARITY_MILLIS;

    fn current_millis(&self) -> u64 {
        self.current
            .load(Ordering::Relaxed)
            .saturating_sub(self.epoch_millis)
            / Self::GRANULARITY_MILLIS
    }
}

impl<const N: u64> TimeSource<u128> for CachedClock<N> {
    const GRANULARITY_MILLIS: NonZeroU64 = Self::GRANULARITY_MILLIS;

    fn current_millis(&self) -> u128 {
        u128::from(<Self as TimeSource<u64>>::current_millis(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_granularity_stays_in_milliseconds() {
        let clock: CachedClock = CachedClock {
            current: Arc::new(AtomicU64::new(42)),
            epoch_millis: 0,
        };
        assert_eq!(<CachedClock as TimeSource<u64>>::current_millis(&clock), 42);
        assert_eq!(CachedClock::<1>::GRANULARITY_MILLIS, NonZeroU64::MIN);
        assert_eq!(
            <CachedClock as TimeSource<u64>>::GRANULARITY_MILLIS,
            NonZeroU64::MIN
        );
    }

    #[test]
    fn applies_epoch_and_quantization_to_latest_wall_time() {
        let clock = CachedClock::<8> {
            current: Arc::new(AtomicU64::new(1024)),
            epoch_millis: 1000,
        };
        assert_eq!(
            <CachedClock<8> as TimeSource<u64>>::current_millis(&clock),
            3
        );
        clock.current.store(1008, Ordering::Relaxed);
        assert_eq!(
            <CachedClock<8> as TimeSource<u64>>::current_millis(&clock),
            1
        );
        clock.current.store(999, Ordering::Relaxed);
        assert_eq!(
            <CachedClock<8> as TimeSource<u64>>::current_millis(&clock),
            0
        );
        assert_eq!(
            <CachedClock<8> as TimeSource<u128>>::current_millis(&clock),
            0
        );
    }

    #[test]
    fn handles_maximum_millisecond_value() {
        let clock = CachedClock::<8> {
            current: Arc::new(AtomicU64::new(u64::MAX)),
            epoch_millis: 0,
        };
        assert_eq!(
            <CachedClock<8> as TimeSource<u64>>::current_millis(&clock),
            u64::MAX / 8
        );
        assert_eq!(
            <CachedClock<8> as TimeSource<u128>>::current_millis(&clock),
            u128::from(u64::MAX / 8)
        );
    }

    #[cfg(all(feature = "basic", feature = "snowflake", feature = "ulid"))]
    #[test]
    fn monotonic_generators_wait_for_backward_clock_correction() {
        use crate::{
            generator::{BasicMonoUlidGenerator, BasicSnowflakeGenerator, Poll},
            id::{SnowflakeTwitterId, ULID},
            rand::RandSource,
        };

        struct ZeroRand;
        impl RandSource<u128> for ZeroRand {
            fn rand(&self) -> u128 {
                0
            }
        }

        let clock = CachedClock::<1> {
            current: Arc::new(AtomicU64::new(1000)),
            epoch_millis: 0,
        };
        let snowflake = BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(1, clock.clone());
        let ulid = BasicMonoUlidGenerator::<ULID, _, _>::new(clock.clone(), ZeroRand);
        let first_snowflake = snowflake.next_id(|_| panic!("expected ready ID"));
        let first_ulid = ulid.next_id(|_| panic!("expected ready ID"));

        clock.current.store(999, Ordering::Relaxed);
        core::assert_matches!(snowflake.poll_id(), Poll::Pending { yield_for } if yield_for == Duration::from_millis(1));
        core::assert_matches!(ulid.poll_id(), Poll::Pending { yield_for } if yield_for == Duration::from_millis(1));

        clock.current.store(1000, Ordering::Relaxed);
        assert!(snowflake.next_id(|_| panic!("expected ready ID")) > first_snowflake);
        assert!(ulid.next_id(|_| panic!("expected ready ID")) > first_ulid);
    }
}
