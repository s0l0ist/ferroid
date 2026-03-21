use core::time::Duration;

/// Unix epoch: Thursday, January 1, 1970 00:00:00 UTC
pub const UNIX_EPOCH: Duration = Duration::from_millis(0);

/// Twitter epoch: Thursday, November 4, 2010 1:42:54.657 UTC
pub const TWITTER_EPOCH: Duration = Duration::from_millis(1_288_834_974_657);

/// Discord epoch: Thursday, January 1, 2015 00:00:00 UTC
pub const DISCORD_EPOCH: Duration = Duration::from_millis(1_420_070_400_000);

/// Instagram epoch: Saturday, January 1, 2011 00:00:00 UTC
pub const INSTAGRAM_EPOCH: Duration = Duration::from_millis(1_293_840_000_000);

/// Mastodon epoch: Thursday, January 1, 1970 00:00:00 UTC
pub const MASTODON_EPOCH: Duration = UNIX_EPOCH;

/// A trait for time sources that return a monotonic or wall-clock timestamp.
///
/// This abstraction allows you to plug in a real system clock, a monotonic
/// timer, or a mocked time source in tests.
///
/// The timestamp type `T` is generic (typically `u64` or `u128`).
///
/// By default, one returned time unit corresponds to one real millisecond.
/// Time sources may override [`GRANULARITY_MILLIS`] to expose coarser clock
/// quanta while still allowing generic code to recover the real duration of a
/// single returned unit.
///
/// # Example
/// ```
/// use ferroid::time::TimeSource;
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
    /// The number of real milliseconds represented by one returned time unit.
    const GRANULARITY_MILLIS: u64 = 1;

    /// Returns the current time since the configured epoch in this source's
    /// native units.
    fn current_millis(&self) -> T;
}
