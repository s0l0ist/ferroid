use core::{cmp, marker::PhantomData, time::Duration};

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{AtomicInteger, Poll, Result, UlidGenerator},
    id::{ToU64, UlidId},
    rand::RandSource,
    time::TimeSource,
};

/// A lock-free *monotonic* ULID-style ID generator suitable for multi-threaded
/// environments.
///
/// The ID's backing type selects the atomic storage: `u64` uses
/// [`portable_atomic::AtomicU64`] and `u128` uses [`portable_atomic::AtomicU128`].
/// Both allow safe shared use across threads with the same generator API.
///
/// ## Features
/// - ✅ Thread-safe
/// - ✅ Probabilistically unique (no coordination required)
/// - ✅ Time-ordered (monotonically increasing per time-source tick)
///
/// ## Recommended When
/// - You're in a multi-threaded environment
/// - You need monotonically increasing IDs (IDs generated within the same
///   time-source tick increment the random component)
///
/// ## See Also
/// - [`BasicUlidGenerator`]
/// - [`BasicMonoUlidGenerator`]
/// - [`LockMonoUlidGenerator`]
///
/// [`BasicUlidGenerator`]: crate::generator::BasicUlidGenerator
/// [`BasicMonoUlidGenerator`]: crate::generator::BasicMonoUlidGenerator
/// [`LockMonoUlidGenerator`]: crate::generator::LockMonoUlidGenerator
pub struct AtomicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    ID::Ty: AtomicInteger,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    state: crossbeam_utils::CachePadded<<ID::Ty as AtomicInteger>::Atomic>,
    time: T,
    rng: R,
    _id: PhantomData<ID>,
}

impl<ID, T, R> AtomicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    ID::Ty: AtomicInteger,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// Creates a new [`AtomicMonoUlidGenerator`] with the provided time source
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
    /// # #[cfg(all(feature = "thread-rng", target_has_atomic = "128"))]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = AtomicMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
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
            state: crossbeam_utils::CachePadded::new(ID::Ty::new_atomic(id.to_raw())),
            time,
            rng,
            _id: PhantomData,
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
    /// # #[cfg(all(feature = "thread-rng", target_has_atomic = "128"))]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = AtomicMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    pub fn next_id(&self, f: impl FnMut(Duration)) -> ID {
        UlidGenerator::next_id(self, f)
    }

    /// Attempts to generate a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the random component is
    /// exhausted and the time has not advanced, or CAS fails), it returns
    /// [`Poll::Pending`].
    ///
    /// Pending delays are [`Duration`] values with clock granularity already
    /// applied. [`Duration::ZERO`] permits an immediate retry.
    ///
    /// # Example
    /// ```
    /// # #[cfg(all(feature = "thread-rng", target_has_atomic = "128"))]
    /// # {
    /// use ferroid::{
    ///     generator::{AtomicMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = AtomicMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
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

        let current_raw = ID::Ty::load(&self.state);
        let current_id = ID::from_raw(current_raw);
        let current_ts = current_id.timestamp();

        let next_id = match now.cmp(&current_ts) {
            cmp::Ordering::Equal => {
                if current_id.has_random_room() {
                    current_id.increment_random()
                } else {
                    return Poll::Pending {
                        yield_for: Duration::from_millis(T::GRANULARITY_MILLIS.get()),
                    };
                }
            }
            cmp::Ordering::Greater => current_id.rollover_to_timestamp(now, self.rng.rand::<ID>()),
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

impl<ID, T, R> UlidGenerator<ID, T, R> for AtomicMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    ID::Ty: AtomicInteger,
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
    use core::time::Duration;

    #[cfg(all(feature = "std", target_has_atomic = "128"))]
    use super::super::tests::MockTime;
    #[cfg(target_has_atomic = "128")]
    use super::super::tests::{CountingRand, OverflowClock, TestClock, ZeroRand};
    use super::*;
    use crate::generator::Poll;
    #[cfg(target_has_atomic = "128")]
    use crate::id::ULID;

    #[cfg(target_has_atomic = "128")]
    #[test]
    fn pending_delays_saturate_at_u64_milliseconds() {
        let atomic = crate::generator::AtomicMonoUlidGenerator::<ULID, _, _>::from_components(
            2,
            0,
            OverflowClock,
            ZeroRand,
        );
        assert_eq!(
            atomic.poll_id(),
            Poll::Pending {
                yield_for: Duration::from_millis(u64::MAX)
            }
        );
    }

    #[cfg(target_has_atomic = "128")]
    #[test]
    fn handles_rollback_and_rollover() {
        let shared_time = TestClock::new(2);
        let generator: AtomicMonoUlidGenerator<ULID, _, _> =
            AtomicMonoUlidGenerator::from_components(2, ULID::max_random(), &shared_time, ZeroRand);
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

    #[cfg(target_has_atomic = "128")]
    #[test]
    fn is_monotonic_across_ticks() {
        let shared_time = TestClock::new(1);
        let generator: AtomicMonoUlidGenerator<ULID, _, _> =
            AtomicMonoUlidGenerator::new(&shared_time, CountingRand::new());
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

    #[cfg(target_has_atomic = "128")]
    #[test]
    fn u128_increments_random_past_u64_max() {
        let clock = TestClock::new(1);
        let generator = AtomicMonoUlidGenerator::<ULID, _, _>::from_components(
            1,
            u128::from(u64::MAX),
            &clock,
            ZeroRand,
        );
        let id = generator.next_id(|_| panic!("expected ready ID"));
        assert_eq!((id.timestamp(), id.random()), (1, u128::from(u64::MAX) + 1));
    }

    #[cfg(all(feature = "std", target_has_atomic = "128"))]
    #[test]
    fn concurrent_uniqueness() {
        use alloc::vec::Vec;
        use std::{sync::Barrier, thread};

        const THREADS: usize = 8;
        const IDS_PER_THREAD: usize = 64;

        let generator =
            AtomicMonoUlidGenerator::<ULID, _, _>::new(MockTime { millis: 1 }, ZeroRand);
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
        for (random, id) in ids.iter().enumerate() {
            assert_eq!(id.timestamp(), 1);
            assert_eq!(id.random(), u128::try_from(random).unwrap());
        }
    }

    #[cfg(target_has_atomic = "64")]
    mod compact {
        use super::super::super::tests::{CountingRand, OverflowClock, TestClock, ZeroRand};
        use super::*;

        crate::define_ulid!(CompactId, u64, reserved: 0, timestamp: 48, random: 16);

        #[test]
        fn pending_delays_saturate_at_u64_milliseconds() {
            let generator = AtomicMonoUlidGenerator::<CompactId, _, _>::from_components(
                2,
                0,
                OverflowClock,
                ZeroRand,
            );
            assert_eq!(
                generator.poll_id(),
                Poll::Pending {
                    yield_for: Duration::from_millis(u64::MAX)
                }
            );
        }

        #[test]
        fn is_monotonic_across_ticks() {
            let clock = TestClock::new(1);
            let generator = AtomicMonoUlidGenerator::new(&clock, CountingRand::new());
            let first: CompactId = generator.next_id(|_| panic!("expected ready ID"));
            let second = generator.next_id(|_| panic!("expected ready ID"));
            assert_eq!((first.timestamp(), first.random()), (1, 0));
            assert_eq!((second.timestamp(), second.random()), (1, 1));
            assert!(second > first);

            clock.ticks.set(2);
            let next = generator.next_id(|_| panic!("expected ready ID"));
            assert_eq!((next.timestamp(), next.random()), (2, 1));
            assert!(next > second);
        }

        #[test]
        fn handles_rollback_and_rollover() {
            let clock = TestClock::new(2);
            let generator = AtomicMonoUlidGenerator::<CompactId, _, _>::from_components(
                2,
                CompactId::max_random(),
                &clock,
                ZeroRand,
            );
            assert_eq!(
                generator.poll_id(),
                Poll::Pending {
                    yield_for: Duration::from_millis(10)
                }
            );

            clock.ticks.set(0);
            assert_eq!(
                generator.poll_id(),
                Poll::Pending {
                    yield_for: Duration::from_millis(20)
                }
            );
            let id = generator.next_id(|delay| {
                assert_eq!(delay, Duration::from_millis(20));
                clock.ticks.set(3);
            });
            assert_eq!((id.timestamp(), id.random()), (3, 0));
        }
    }
}
