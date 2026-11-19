use core::{cell::Cell, cmp::Ordering, time::Duration};

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{Poll, Result, UlidGenerator},
    id::{ToU64, UlidId},
    rand::RandSource,
    time::TimeSource,
};

/// A *monotonic* ULID-style ID generator suitable for single-threaded
/// environments.
///
/// This generator is lightweight and fast, but is not thread-safe.
///
/// ## Features
/// - ❌ Not thread-safe
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (monotonically increasing per time-source tick)
///
/// ## Recommended When
/// - You're in a single-threaded environment (no shared access)
/// - You need monotonically increasing IDs (IDs generated within the same
///   time-source tick increment the random component)
///
/// ## See Also
/// - [`BasicUlidGenerator`]
/// - [`LockMonoUlidGenerator`]
/// - [`AtomicMonoUlidGenerator`]
///
/// [`BasicUlidGenerator`]: crate::generator::BasicUlidGenerator
/// [`AtomicMonoUlidGenerator`]: crate::generator::AtomicMonoUlidGenerator
/// [`LockMonoUlidGenerator`]: crate::generator::LockMonoUlidGenerator
pub struct BasicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    state: Cell<ID>,
    time: T,
    rng: R,
}

impl<ID, T, R> BasicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// Creates a new [`BasicMonoUlidGenerator`] with the provided time source
    /// and RNG.
    ///
    /// # Parameters
    /// - `time`: A [`TimeSource`] used to retrieve the current timestamp
    /// - `rng`: A [`RandSource`] used to generate random bits
    ///
    /// # Returns
    /// A ready-to-use ULID generator suitable for producing unique, sortable
    /// IDs.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "thread-rng")]
    /// # {
    /// use ferroid::{
    ///     generator::{BasicMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = BasicMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    ///
    /// [`TimeSource`]: crate::time::TimeSource
    /// [`RandSource`]: crate::rand::RandSource
    pub fn new(time: T, rng: R) -> Self {
        Self::from_components(ID::ZERO, ID::ZERO, time, rng)
    }

    /// Creates a new ID generator from explicit component values.
    ///
    /// This constructor is primarily useful for advanced use cases such as
    /// restoring state from persistent storage or controlling the starting
    /// point of the generator manually.
    ///
    /// # Parameters
    /// - `timestamp`: The initial timestamp component (usually in
    ///   time-source units)
    /// - `random`: The initial random component
    /// - `time`: A [`TimeSource`] implementation used to fetch the current time
    /// - `rng`: A [`RandSource`] used to generate future random bits
    ///
    /// # Returns
    /// A new generator instance preloaded with the given state.
    ///
    /// # ⚠️ Note
    /// In typical use cases, you should prefer [`Self::new`] to let the
    /// generator initialize itself from the current time.
    pub fn from_components(timestamp: ID::Ty, random: ID::Ty, time: T, rng: R) -> Self {
        let id = ID::from_components(timestamp, random);
        Self {
            state: Cell::new(id),
            time,
            rng,
        }
    }

    /// Generates a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID.
    ///
    /// When generation is pending, `f` receives a suggested [`Duration`],
    /// with clock granularity already applied. The delay may be zero and
    /// saturates at `u64::MAX` milliseconds.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "thread-rng")]
    /// # {
    /// use ferroid::{
    ///     generator::{BasicMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = BasicMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    pub fn next_id(&self, f: impl FnMut(Duration)) -> ID {
        UlidGenerator::next_id(self, f)
    }

    /// Generates a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the random component is
    /// exhausted and the time has not advanced), it returns [`Poll::Pending`].
    ///
    /// Pending delays are [`Duration`] values with clock granularity already
    /// applied. [`Duration::ZERO`] permits an immediate retry.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "thread-rng")]
    /// # {
    /// use ferroid::{
    ///     generator::{BasicMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = BasicMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = loop {
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
                if state.has_random_room() {
                    let updated = state.increment_random();
                    self.state.set(updated);
                    Poll::Ready { id: updated }
                } else {
                    Poll::Pending {
                        yield_for: Duration::from_millis(T::GRANULARITY_MILLIS.get()),
                    }
                }
            }
            Ordering::Greater => {
                // Set the new timestamp and random number.
                let rand = self.rng.rand::<ID>();
                let updated = state.rollover_to_timestamp(now, rand);
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

impl<ID, T, R> UlidGenerator<ID, T, R> for BasicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    type Err = core::convert::Infallible;

    fn new(time: T, rng: R) -> Self {
        Self::new(time, rng)
    }

    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err> {
        Ok(self.poll_id())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{CountingRand, MockTime, OverflowClock, TestClock, ZeroRand};
    use super::*;
    use crate::generator::Poll;
    use crate::id::ULID;
    use core::time::Duration;

    #[test]
    fn pending_delays_saturate_at_u64_milliseconds() {
        let basic =
            BasicMonoUlidGenerator::<ULID, _, _>::from_components(2, 0, OverflowClock, ZeroRand);
        assert_eq!(
            basic.poll_id(),
            Poll::Pending {
                yield_for: Duration::from_millis(u64::MAX)
            }
        );
    }

    #[test]
    fn pending_delay_caps_wide_timestamps() {
        crate::define_ulid!(WideTimestampId, u128, reserved: 0, timestamp: 128, random: 0);
        let timestamp = u128::from(u64::MAX) + 1;
        let generator = BasicMonoUlidGenerator::<WideTimestampId, _, _>::from_components(
            timestamp,
            0,
            MockTime { millis: 0 },
            ZeroRand,
        );
        assert_eq!(
            generator.poll_id(),
            Poll::Pending {
                yield_for: Duration::from_millis(u64::MAX)
            }
        );

        let overflowing = BasicMonoUlidGenerator::<WideTimestampId, _, _>::from_components(
            u128::MAX,
            0,
            OverflowClock,
            ZeroRand,
        );
        assert_eq!(
            overflowing.poll_id(),
            Poll::Pending {
                yield_for: Duration::from_millis(u64::MAX)
            }
        );
    }

    #[test]
    fn handles_rollback_and_rollover() {
        let shared_time = TestClock::new(2);
        let generator: BasicMonoUlidGenerator<ULID, _, _> =
            BasicMonoUlidGenerator::from_components(2, ULID::max_random(), &shared_time, ZeroRand);
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
        assert_eq!((id.timestamp(), id.random()), (3, 0));
    }

    #[test]
    fn is_monotonic_across_ticks() {
        let shared_time = TestClock::new(1);
        let generator: BasicMonoUlidGenerator<ULID, _, _> =
            BasicMonoUlidGenerator::new(&shared_time, CountingRand::new());
        // Two IDs in one tick increment the random field.
        let first = generator.next_id(|_| panic!("expected ready ID"));
        let second = generator.next_id(|_| panic!("expected ready ID"));
        assert_eq!((first.timestamp(), first.random()), (1, 0));
        assert_eq!((second.timestamp(), second.random()), (1, 1));
        assert!(second > first);

        shared_time.ticks.set(2);
        let next = generator.next_id(|_| panic!("expected ready ID"));
        assert_eq!((next.timestamp(), next.random()), (2, 1));
        assert!(next > second);
    }
}
