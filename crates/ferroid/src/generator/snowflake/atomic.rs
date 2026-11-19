use core::{cmp, marker::PhantomData, time::Duration};

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{AtomicInteger, Poll, Result, SnowflakeGenerator},
    id::{SnowflakeId, ToU64},
    time::TimeSource,
};

/// A lock-free Snowflake ID generator suitable for multi-threaded environments.
///
/// The ID's backing type selects the atomic storage: `u64` uses
/// [`portable_atomic::AtomicU64`] and `u128` uses [`portable_atomic::AtomicU128`].
/// Both allow safe shared use across threads with the same generator API.
///
/// ## Features
/// - ✅ Thread-safe
/// - ✅ Custom [`SnowflakeId`] layouts backed by `u64` or `u128`
///
/// ## Recommended When
/// - You're in a multi-threaded environment
/// - Fair access is sacrificed for higher throughput
///
/// ## See Also
/// - [`BasicSnowflakeGenerator`]
/// - [`LockSnowflakeGenerator`]
///
/// [`BasicSnowflakeGenerator`]: crate::generator::BasicSnowflakeGenerator
/// [`LockSnowflakeGenerator`]: crate::generator::LockSnowflakeGenerator
pub struct AtomicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    ID::Ty: AtomicInteger,
    T: TimeSource<ID::Ty>,
{
    state: crossbeam_utils::CachePadded<<ID::Ty as AtomicInteger>::Atomic>,
    time: T,
    _id: PhantomData<ID>,
}

impl<ID, T> AtomicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    ID::Ty: AtomicInteger,
    T: TimeSource<ID::Ty>,
{
    /// Creates a new [`AtomicSnowflakeGenerator`] initialized with the current
    /// time and a given machine ID.
    ///
    /// This constructor sets the initial timestamp and sequence to zero, and
    /// uses the provided `time` to fetch the current time during ID generation.
    /// It is the recommended way to create a new atomic generator for typical
    /// use cases.
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
    /// A new [`AtomicSnowflakeGenerator`] ready to produce unique, time-ordered
    /// IDs.
    ///
    /// # Example
    /// ```
    /// # #[cfg(all(feature = "std", target_has_atomic = "64"))]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{CachedClock, TWITTER_EPOCH},
    /// };
    ///
    /// let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
    /// let generator = AtomicSnowflakeGenerator::new(0, clock);
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
        let initial = ID::from_components(timestamp, machine_id, sequence);
        Self {
            state: crossbeam_utils::CachePadded::new(ID::Ty::new_atomic(initial.to_raw())),
            time,
            _id: PhantomData,
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
    /// # #[cfg(all(feature = "std", target_has_atomic = "64"))]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = AtomicSnowflakeGenerator::new(0, CachedClock::default());
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
    /// time has not advanced, or CAS fails), it returns [`Poll::Pending`].
    ///
    /// Pending delays are [`Duration`] values with clock granularity already
    /// applied. [`Duration::ZERO`] permits an immediate retry.
    ///
    /// # Example
    /// ```
    /// # #[cfg(all(feature = "std", target_has_atomic = "64"))]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{CachedClock, TWITTER_EPOCH},
    /// };
    ///
    /// let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
    /// let generator = AtomicSnowflakeGenerator::new(0, clock);
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

        let current_raw = ID::Ty::load(&self.state);
        let current_id = ID::from_raw(current_raw);
        let current_ts = current_id.timestamp();

        let next_id = match now.cmp(&current_ts) {
            cmp::Ordering::Equal => {
                if current_id.has_sequence_room() {
                    current_id.increment_sequence()
                } else {
                    return Poll::Pending {
                        yield_for: Duration::from_millis(T::GRANULARITY_MILLIS.get()),
                    };
                }
            }
            cmp::Ordering::Greater => current_id.rollover_to_timestamp(now),
            cmp::Ordering::Less => {
                return Self::cold_clock_behind(now, current_ts);
            }
        };

        let next_raw = next_id.to_raw();

        if ID::Ty::compare_exchange(&self.state, current_raw, next_raw).is_ok() {
            Poll::Ready { id: next_id }
        } else {
            // CAS failed - another thread won the race. Yield 0 to retry
            // immediately.
            Poll::Pending {
                yield_for: Duration::ZERO,
            }
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

impl<ID, T> SnowflakeGenerator<ID, T> for AtomicSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    ID::Ty: AtomicInteger,
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
    use core::time::Duration;

    #[cfg(all(feature = "std", target_has_atomic = "64"))]
    use super::super::tests::MockTime;
    #[cfg(target_has_atomic = "64")]
    use super::super::tests::{OverflowClock, TestClock};
    use super::*;
    use crate::generator::Poll;
    #[cfg(target_has_atomic = "64")]
    use crate::id::SnowflakeTwitterId;

    #[cfg(target_has_atomic = "64")]
    #[test]
    fn pending_delays_saturate_at_u64_milliseconds() {
        let atomic =
            crate::generator::AtomicSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(
                2,
                0,
                0,
                OverflowClock,
            );
        assert_eq!(
            atomic.poll_id(),
            Poll::Pending {
                yield_for: Duration::from_millis(u64::MAX)
            }
        );
    }

    #[cfg(target_has_atomic = "64")]
    #[test]
    fn handles_rollback_and_rollover() {
        let shared_time = TestClock::new(2);
        let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
            AtomicSnowflakeGenerator::from_components(
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

    #[cfg(target_has_atomic = "64")]
    #[test]
    fn is_monotonic_across_ticks() {
        let shared_time = TestClock::new(1);
        let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
            AtomicSnowflakeGenerator::new(1, &shared_time);
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

    #[cfg(all(feature = "std", target_has_atomic = "64"))]
    #[test]
    fn concurrent_uniqueness() {
        use alloc::vec::Vec;
        use std::{sync::Barrier, thread};

        const THREADS: usize = 8;
        // The whole batch fits within one Snowflake timestamp's sequence range.
        const IDS_PER_THREAD: usize = 64;

        let generator =
            AtomicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, MockTime { millis: 1 });
        let barrier = Barrier::new(THREADS);
        let mut ids = Vec::with_capacity(THREADS * IDS_PER_THREAD);

        thread::scope(|s| {
            let mut handles = Vec::with_capacity(THREADS);
            for _ in 0..THREADS {
                handles.push(s.spawn(|| {
                    barrier.wait();
                    let mut ids = Vec::with_capacity(IDS_PER_THREAD);
                    for _ in 0..IDS_PER_THREAD {
                        let id = generator
                            .try_next_id(|yield_for| {
                                // Only a lost CAS may be pending at this fixed timestamp.
                                assert_eq!(yield_for, Duration::ZERO);
                            })
                            .unwrap();
                        ids.push(id);
                    }
                    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
                    ids
                }));
            }
            for handle in handles {
                ids.extend(handle.join().unwrap());
            }
        });

        ids.sort_unstable();
        assert_eq!(ids.len(), THREADS * IDS_PER_THREAD);
        for (sequence, id) in ids.iter().enumerate() {
            assert_eq!(id.timestamp(), 1);
            assert_eq!(id.machine_id(), 0);
            assert_eq!(id.sequence(), u64::try_from(sequence).unwrap());
        }
    }

    #[cfg(target_has_atomic = "128")]
    mod wide {
        use super::super::super::tests::TestClock;
        use super::*;

        crate::define_snowflake_id!(WideId, u128,
            reserved: 0, timestamp: 80, machine_id: 16, sequence: 32);
        crate::define_snowflake_id!(WideSequenceId, u128,
            reserved: 0, timestamp: 16, machine_id: 16, sequence: 96);

        #[test]
        fn is_monotonic_across_ticks() {
            let timestamp = u128::from(u64::MAX) + 42;
            let clock = TestClock::new(timestamp);
            let generator = AtomicSnowflakeGenerator::new(7, &clock);
            let first: WideId = generator.next_id(|_| panic!("expected ready ID"));
            let second = generator.next_id(|_| panic!("expected ready ID"));
            assert_eq!(
                (first.timestamp(), first.machine_id(), first.sequence()),
                (timestamp, 7, 0)
            );
            assert_eq!(
                (second.timestamp(), second.machine_id(), second.sequence()),
                (timestamp, 7, 1)
            );
            assert!(second > first);

            clock.ticks.set(timestamp + 1);
            let next = generator.next_id(|_| panic!("expected ready ID"));
            assert_eq!(
                (next.timestamp(), next.machine_id(), next.sequence()),
                (timestamp + 1, 7, 0)
            );
            assert!(next > second);
        }

        #[test]
        fn handles_rollback_and_rollover() {
            let timestamp = u128::from(u64::MAX) + 42;
            let clock = TestClock::new(timestamp);
            let generator = AtomicSnowflakeGenerator::<WideId, _>::from_components(
                timestamp,
                7,
                WideId::max_sequence(),
                &clock,
            );
            assert_eq!(
                generator.poll_id(),
                Poll::Pending {
                    yield_for: Duration::from_millis(10)
                }
            );

            clock.ticks.set(timestamp - 2);
            assert_eq!(
                generator.poll_id(),
                Poll::Pending {
                    yield_for: Duration::from_millis(20)
                }
            );
            let id = generator.next_id(|delay| {
                assert_eq!(delay, Duration::from_millis(20));
                clock.ticks.set(timestamp + 1);
            });
            assert_eq!(
                (id.timestamp(), id.machine_id(), id.sequence()),
                (timestamp + 1, 7, 0)
            );
        }

        #[test]
        fn pending_delays_saturate_at_u64_milliseconds() {
            let clock = TestClock::new(0);
            let generator = AtomicSnowflakeGenerator::<WideId, _>::from_components(
                u128::from(u64::MAX) + 42,
                7,
                0,
                &clock,
            );
            assert_eq!(
                generator.poll_id(),
                Poll::Pending {
                    yield_for: Duration::from_millis(u64::MAX)
                }
            );
        }

        #[test]
        fn sequence_increments_past_u64_max() {
            let clock = TestClock::new(1);
            let generator = AtomicSnowflakeGenerator::<WideSequenceId, _>::from_components(
                1,
                7,
                u128::from(u64::MAX),
                &clock,
            );
            let id = generator.next_id(|_| panic!("expected ready ID"));
            assert_eq!(
                (id.timestamp(), id.machine_id(), id.sequence()),
                (1, 7, u128::from(u64::MAX) + 1)
            );
        }
    }
}
