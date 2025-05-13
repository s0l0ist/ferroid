use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// A monotonic time source that returns elapsed time since process start,
/// offset from a user-defined epoch.
///
/// This avoids wall-clock adjustments (e.g., NTP or daylight savings changes)
/// while still aligning timestamps to a fixed origin.
///
/// Internally, the clock measures time by capturing `Instant::now()` at
/// construction and adding to it the duration elapsed since a given epoch
/// (computed from `SystemTime::now()` at startup).
#[derive(Copy, Clone)]
pub struct MonotonicClock {
    epoch: Instant,
    offset: Duration,
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
    /// This epoch defines the zero-point for all future timestamps returned by
    /// this clock. Internally, the clock measures elapsed time using a
    /// monotonic counter and adds it to the offset between `SystemTime::now()`
    /// and the given epoch.
    ///
    /// # Parameters
    ///
    /// - `epoch`: The origin timestamp, as a [`Duration`] since 1970-01-01 UTC.
    ///
    /// # Panics
    ///
    /// Panics if the current system time is earlier than the given epoch.
    ///
    /// # Example
    ///
    /// ```
    /// use ferroid::{MonotonicClock, TimeSource};
    /// let now = std::time::SystemTime::now()
    ///     .duration_since(std::time::UNIX_EPOCH)
    ///     .unwrap();
    ///
    /// // Or use a default epoch
    /// // use ferroid::TWITTER_EPOCH,
    /// // let now = TWITTER_EPOCH;
    ///
    /// let clock = MonotonicClock::with_epoch(now);
    /// let ts = clock.current_millis();
    /// assert_eq!(ts, 0);
    /// ```
    ///
    /// This allows you to control the timestamp layout (e.g., Snowflake-style
    /// ID encoding) by anchoring all generated times to a custom epoch of your
    /// choosing.
    pub fn with_epoch(epoch: Duration) -> Self {
        let offset = SystemTime::now()
            .duration_since(UNIX_EPOCH + epoch)
            .expect("System time before custom epoch");

        Self {
            epoch: Instant::now(),
            offset,
        }
    }
}

impl TimeSource<u64> for MonotonicClock {
    /// Returns the number of milliseconds since the configured epoch, based on
    /// the elapsed monotonic time since construction.
    fn current_millis(&self) -> u64 {
        (self.offset + self.epoch.elapsed()).as_millis() as u64
    }
}
