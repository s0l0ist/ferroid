use core::{cell::Cell, cmp::Ordering, time::Duration};

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{Poll, Result, SnowflakeGenerator},
    id::{SnowflakeId, ToU64},
    time::TimeSource,
};

/// A non-concurrent Snowflake ID generator suitable for single-threaded
/// environments.
///
/// This generator is lightweight and fast, but **not thread-safe**.
///
/// ## Features
/// - ❌ Not thread-safe
/// - ✅ Safely implement any [`SnowflakeId`] layout
///
/// ## Recommended When
/// - You're in a single-threaded environment (no shared access)
/// - You want the fastest generator
///
/// ## See Also
/// - [`LockSnowflakeGenerator`]
/// - [`AtomicSnowflakeGenerator`]
///
/// [`LockSnowflakeGenerator`]: crate::generator::LockSnowflakeGenerator
/// [`AtomicSnowflakeGenerator`]: crate::generator::AtomicSnowflakeGenerator
pub struct BasicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    state: Cell<ID>,
    time: T,
}

impl<ID, T> BasicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    /// Creates a new [`BasicSnowflakeGenerator`] initialized with the current
    /// time and a given machine ID.
    ///
    /// This constructor sets the initial timestamp and sequence to zero, and
    /// uses the provided `time` to fetch the current time during ID generation.
    /// It is the recommended way to create a new generator for typical use
    /// cases.
    ///
    /// # Parameters
    ///
    /// - `machine_id`: A unique identifier for the node or instance generating
    ///   IDs. This value will be encoded into every generated ID.
    /// - `time`: A [`TimeSource`] implementation (e.g., [`CachedClock`])
    ///   that determines how timestamps are generated.
    ///
    /// # Returns
    ///
    /// A new [`BasicSnowflakeGenerator`] ready to produce unique, time-ordered
    /// IDs.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "std")]
    /// # {
    /// use ferroid::{
    ///     generator::{BasicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{CachedClock, TWITTER_EPOCH},
    /// };
    ///
    /// let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
    /// let generator = BasicSnowflakeGenerator::new(0, clock);
    ///
    /// let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    ///
    /// [`TimeSource`]: crate::time::TimeSource
    /// [`CachedClock`]: crate::time::CachedClock
    pub fn new(machine_id: ID::Ty, time: T) -> Self {
        Self::from_components(ID::ZERO, machine_id, ID::ZERO, time)
    }

    /// Creates a new ID generator from explicit component values.
    ///
    /// This constructor is primarily useful for advanced use cases such as
    /// restoring state from persistent storage or controlling the starting
    /// point of the generator manually.
    ///
    /// # Parameters
    /// - `timestamp`: The initial timestamp component (usually in time-source units)
    /// - `machine_id`: The machine or worker identifier
    /// - `sequence`: The initial sequence number
    /// - `time`: A [`TimeSource`] implementation used to fetch the current time
    ///
    /// # Returns
    /// A new generator instance preloaded with the given state.
    ///
    /// # ⚠️ Note
    /// In typical use cases, you should prefer [`Self::new`] to let the
    /// generator initialize itself from the current time.
    pub fn from_components(
        timestamp: ID::Ty,
        machine_id: ID::Ty,
        sequence: ID::Ty,
        time: T,
    ) -> Self {
        let id = ID::from_components(timestamp, machine_id, sequence);
        Self {
            state: Cell::new(id),
            time,
        }
    }

    /// Generates a new ID.
    ///
    /// Returns a new, time-ordered, unique ID.
    ///
    /// When generation is pending, `f` receives a suggested [`Duration`],
    /// with clock granularity already applied. The delay may be zero and
    /// saturates at `u64::MAX` milliseconds.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "std")]
    /// # {
    /// use ferroid::{
    ///     generator::{BasicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = BasicSnowflakeGenerator::new(0, CachedClock::default());
    ///
    /// let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    pub fn next_id(&self, f: impl FnMut(Duration)) -> ID {
        SnowflakeGenerator::next_id(self, f)
    }

    /// Attempts to generate the next available ID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the sequence is full and the
    /// time has not advanced), it returns [`Poll::Pending`].
    ///
    /// Pending delays are [`Duration`] values with clock granularity already
    /// applied. [`Duration::ZERO`] permits an immediate retry.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "std")]
    /// # {
    /// use ferroid::{
    ///     generator::{BasicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{CachedClock, TWITTER_EPOCH},
    /// };
    ///
    /// let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
    /// let generator = BasicSnowflakeGenerator::new(0, clock);
    ///
    /// let id: SnowflakeTwitterId = loop {
    ///     match generator.poll_id() {
    ///         Poll::Ready { id } => break id,
    ///         Poll::Pending { .. } => std::thread::yield_now(),
    ///     }
    /// };
    /// # }
    /// ```
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn poll_id(&self) -> Poll<ID> {
        let now = self.time.current_millis();
        let state = self.state.get();
        let current_ts = state.timestamp();

        match now.cmp(&current_ts) {
            Ordering::Equal => {
                if state.has_sequence_room() {
                    let updated = state.increment_sequence();
                    self.state.set(updated);
                    Poll::Ready { id: updated }
                } else {
                    Poll::Pending {
                        yield_for: Duration::from_millis(T::GRANULARITY_MILLIS.get()),
                    }
                }
            }
            Ordering::Greater => {
                let updated = state.rollover_to_timestamp(now);
                self.state.set(updated);
                Poll::Ready { id: updated }
            }
            Ordering::Less => Self::cold_clock_behind(now, current_ts),
        }
    }

    #[cold]
    #[inline(never)]
    fn cold_clock_behind(now: ID::Ty, current_ts: ID::Ty) -> Poll<ID> {
        let units = current_ts - now;
        debug_assert!(units >= ID::ZERO);
        Poll::Pending {
            yield_for: Duration::from_millis(
                units.to_u64().saturating_mul(T::GRANULARITY_MILLIS.get()),
            ),
        }
    }
}

impl<ID, T> SnowflakeGenerator<ID, T> for BasicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err = core::convert::Infallible;

    fn new(machine_id: ID::Ty, time: T) -> Self {
        Self::new(machine_id, time)
    }

    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err> {
        Ok(self.poll_id())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{MockTime, OverflowClock, TestClock};
    use super::*;
    use crate::generator::Poll;
    use crate::id::SnowflakeTwitterId;
    use core::num::NonZeroU64;
    use core::time::Duration;

    #[test]
    #[allow(
        dead_code,
        clippy::wrong_self_convention,
        reason = "The layout fixture generates a complete ID API; this test checks wait range."
    )]
    fn pending_delays_exceed_small_backing_types() {
        crate::define_snowflake_id!(SmallId, u32,
            reserved: 0, timestamp: 32, machine_id: 0, sequence: 0);
        struct SlowClock;
        impl TimeSource<u32> for SlowClock {
            const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(1_u64 << u32::BITS).unwrap();

            fn current_millis(&self) -> u32 {
                0
            }
        }

        for (timestamp, expected_millis) in [(0, 1_u64 << 32), (2, 1_u64 << 33)] {
            let expected = Duration::from_millis(expected_millis);
            let generator =
                BasicSnowflakeGenerator::<SmallId, _>::from_components(timestamp, 0, 0, SlowClock);
            assert_eq!(
                generator.poll_id(),
                Poll::Pending {
                    yield_for: expected
                }
            );
        }
    }

    #[test]
    fn pending_delays_saturate_at_u64_milliseconds() {
        let basic = BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(
            2,
            0,
            0,
            OverflowClock,
        );
        assert_eq!(
            basic.poll_id(),
            Poll::Pending {
                yield_for: Duration::from_millis(u64::MAX)
            }
        );
    }

    #[test]
    #[allow(
        dead_code,
        clippy::wrong_self_convention,
        reason = "Layout fixtures generate complete ID APIs; this test exercises wide pending delays."
    )]
    fn pending_delay_caps_wide_timestamps() {
        crate::define_snowflake_id!(WideTimestampId, u128,
            reserved: 0, timestamp: 128, machine_id: 0, sequence: 0);
        let timestamp = u128::from(u64::MAX) + 1;
        let generator = BasicSnowflakeGenerator::<WideTimestampId, _>::from_components(
            timestamp,
            0,
            0,
            MockTime { millis: 0 },
        );
        assert_eq!(
            generator.poll_id(),
            Poll::Pending {
                yield_for: Duration::from_millis(u64::MAX)
            }
        );
    }

    #[test]
    fn handles_rollback_and_rollover() {
        let shared_time = TestClock::new(2);
        let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
            BasicSnowflakeGenerator::from_components(
                2,
                1,
                SnowflakeTwitterId::max_sequence(),
                &shared_time,
            );
        // Exhaustion waits for the next tick: one tick is 10 milliseconds.
        assert_eq!(
            generator.poll_id(),
            Poll::Pending {
                yield_for: Duration::from_millis(10)
            }
        );

        // Rolling back to zero puts the clock two ticks behind the stored timestamp.
        shared_time.ticks.set(0);
        assert_eq!(
            generator.poll_id(),
            Poll::Pending {
                yield_for: Duration::from_millis(20)
            }
        );
        let id = generator.next_id(|delay| {
            assert_eq!(delay, Duration::from_millis(20));
            shared_time.ticks.set(3);
        });
        assert_eq!((id.timestamp(), id.sequence()), (3, 0));
    }

    #[test]
    fn is_monotonic_across_ticks() {
        let shared_time = TestClock::new(1);
        let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
            BasicSnowflakeGenerator::new(1, &shared_time);
        // Two IDs in one tick increment the sequence.
        let first = generator.next_id(|_| panic!("expected ready ID"));
        let second = generator.next_id(|_| panic!("expected ready ID"));
        assert_eq!((first.timestamp(), first.sequence()), (1, 0));
        assert_eq!((second.timestamp(), second.sequence()), (1, 1));
        assert!(second > first);

        shared_time.ticks.set(2);
        let next = generator.next_id(|_| panic!("expected ready ID"));
        assert_eq!((next.timestamp(), next.sequence()), (2, 0));
        assert!(next > second);
    }
}
