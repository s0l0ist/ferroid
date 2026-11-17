use core::{num::NonZeroU64, time::Duration};

use crate::time::{TimeSource, UNIX_EPOCH, system_time::system_time_now};

/// A wall-clock time source that reads the current time on every call.
///
/// Unlike [`CachedClock`], this clock starts no background thread and caches no
/// timestamp. It uses `std::time::SystemTime` by default. On
/// `wasm32-unknown-unknown`, it uses `web-time` to read browser wall time.
///
/// Wall-clock adjustments can move timestamps backwards. Snowflake and
/// monotonic ULID generators return [`Poll::Pending`] while the clock is behind
/// their last generated timestamp.
///
/// `N` sets the number of real milliseconds represented by one returned unit.
/// `DirectClock<1>` returns milliseconds; `DirectClock<8>` returns 8 ms units.
///
/// # Granularity
///
/// A zero granularity is rejected at compile time.
///
/// ```compile_fail
/// use ferroid::time::{DirectClock, UNIX_EPOCH};
/// let _ = DirectClock::<0>::with_epoch(UNIX_EPOCH);
/// ```
///
/// # Example
/// ```
/// use ferroid::time::{DirectClock, TimeSource};
/// let clock = DirectClock::default();
/// let _ms: u64 = clock.current_millis();
/// ```
///
/// [`CachedClock`]: crate::time::CachedClock
/// [`Poll::Pending`]: crate::generator::Poll::Pending
#[derive(Clone, Debug)]
pub struct DirectClock<const N: u64 = 1> {
    epoch_millis: u64,
}

impl Default for DirectClock<1> {
    fn default() -> Self {
        Self::with_epoch(UNIX_EPOCH)
    }
}

impl<const N: u64> DirectClock<N> {
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
            epoch_millis: u64::try_from(epoch.as_millis()).unwrap_or(u64::MAX),
        }
    }

    fn current_units(&self) -> u64 {
        let now = system_time_now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        let now_millis = u64::try_from(now.as_millis()).unwrap_or(u64::MAX);
        now_millis.saturating_sub(self.epoch_millis) / Self::GRANULARITY_MILLIS
    }
}

impl<const N: u64> TimeSource<u64> for DirectClock<N> {
    const GRANULARITY_MILLIS: NonZeroU64 = Self::GRANULARITY_MILLIS;

    fn current_millis(&self) -> u64 {
        self.current_units()
    }
}

impl<const N: u64> TimeSource<u128> for DirectClock<N> {
    const GRANULARITY_MILLIS: NonZeroU64 = Self::GRANULARITY_MILLIS;

    fn current_millis(&self) -> u128 {
        u128::from(self.current_units())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_granularity_stays_in_milliseconds() {
        assert_eq!(DirectClock::<1>::GRANULARITY_MILLIS, NonZeroU64::MIN);
        assert_eq!(
            <DirectClock as TimeSource<u64>>::GRANULARITY_MILLIS,
            NonZeroU64::MIN
        );
    }

    #[test]
    fn reports_wall_clock_millis() {
        let clock = DirectClock::default();
        let millis: u64 = clock.current_millis();
        let system_millis = u64::try_from(
            system_time_now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis(),
        )
        .unwrap();
        assert!(
            system_millis.abs_diff(millis) <= 50,
            "millis diff too large"
        );
    }

    #[test]
    fn quantized_granularity_scales_current_millis() {
        let ms_clock = DirectClock::default();
        let quantized_clock = DirectClock::<8>::with_epoch(UNIX_EPOCH);
        let lower: u64 = ms_clock.current_millis();
        let quantized: u64 = quantized_clock.current_millis();
        let upper: u64 = ms_clock.current_millis();
        assert!(lower / 8 <= quantized);
        assert!(quantized <= upper / 8);
    }

    #[test]
    fn clamps_time_before_epoch_to_zero() {
        let clock = DirectClock::with_epoch(Duration::MAX);
        assert_eq!(<DirectClock as TimeSource<u64>>::current_millis(&clock), 0);
        assert_eq!(<DirectClock as TimeSource<u128>>::current_millis(&clock), 0);
    }
}
