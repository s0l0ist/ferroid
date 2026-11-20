use alloc::sync::Arc;
use core::{cmp::Ordering, convert::Infallible, time::Duration};

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{LockError, Mutex, Poll, Result, UlidGenerator},
    id::{ToU64, UlidId},
    rand::RandSource,
    time::TimeSource,
};

/// A lock-based *monotonic* ULID-style ID generator suitable for multi-threaded
/// environments.
///
/// This generator wraps the ULID state in an [`Arc<Mutex<_>>`], allowing safe
/// shared use across threads.
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
/// - Your target doesn't support atomics.
///
/// ## See Also
/// - [`BasicUlidGenerator`]
/// - [`BasicMonoUlidGenerator`]
/// - [`AtomicMonoUlidGenerator`]
///
/// [`BasicUlidGenerator`]: crate::generator::BasicUlidGenerator
/// [`BasicMonoUlidGenerator`]: crate::generator::BasicMonoUlidGenerator
/// [`AtomicMonoUlidGenerator`]: crate::generator::AtomicMonoUlidGenerator
pub struct LockMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    pub(crate) state: Arc<crossbeam_utils::CachePadded<Mutex<ID>>>,
    pub(crate) time: T,
    pub(crate) rng: R,
}

impl<ID, T, R> LockMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// Creates a new [`LockMonoUlidGenerator`] with the provided time source
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
    /// # #[cfg(all(feature = "parking-lot", feature = "thread-rng"))] {
    ///     use ferroid::{
    ///         generator::{Poll, LockMonoUlidGenerator},
    ///         id::ULID,
    ///         rand::ThreadRandom,
    ///         time::CachedClock,
    ///     };
    ///
    ///     let generator = LockMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    ///     let id: ULID = generator.next_id(|_| std::thread::yield_now());
    /// }
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
            state: Arc::new(crossbeam_utils::CachePadded::new(Mutex::new(id))),
            time,
            rng,
        }
    }

    /// Generates a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID.
    ///
    /// Requires an infallible mutex backend (`parking-lot`).
    ///
    /// When generation is pending, `f` receives a suggested [`Duration`],
    /// with clock granularity already applied. The delay may be zero and
    /// saturates at `u64::MAX` milliseconds.
    ///
    /// # Example
    /// ```
    /// # #[cfg(all(feature = "parking-lot", feature = "thread-rng"))] {
    /// use ferroid::{
    ///     generator::{LockMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = LockMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    pub fn next_id(&self, f: impl FnMut(Duration)) -> ID
    where
        Self: UlidGenerator<ID, T, R, Err: Into<Infallible>>,
    {
        UlidGenerator::next_id(self, f)
    }

    /// Generates a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID with fallible error handling.
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
    ///     generator::{LockMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = LockMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = match generator.try_next_id(|_| std::thread::yield_now()) {
    ///     Ok(id) => id,
    ///     Err(_) => unreachable!(),
    /// };
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the generator fails, such as from lock poisoning.
    pub fn try_next_id(&self, f: impl FnMut(Duration)) -> Result<ID, LockError> {
        UlidGenerator::try_next_id(self, f)
    }

    /// Generates a new ULID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the random component is
    /// exhausted and the time has not advanced), it returns [`Poll::Pending`].
    ///
    /// Requires an infallible mutex backend (`parking-lot`).
    ///
    /// Pending delays are [`Duration`] values with clock granularity already
    /// applied. [`Duration::ZERO`] permits an immediate retry.
    ///
    /// # Example
    /// ```
    /// # #[cfg(all(feature = "parking-lot", feature = "thread-rng"))] {
    /// use ferroid::{
    ///     generator::{LockMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = LockMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// let id: ULID = loop {
    ///     match generator.poll_id() {
    ///         Poll::Ready { id } => break id,
    ///         Poll::Pending { .. } => std::thread::yield_now(),
    ///     }
    /// };
    /// # }
    /// ```
    pub fn poll_id(&self) -> Poll<ID>
    where
        Self: UlidGenerator<ID, T, R, Err: Into<Infallible>>,
    {
        UlidGenerator::poll_id(self)
    }

    /// Attempts to generate a new ULID with fallible error handling.
    ///
    /// This method attempts to generate the next ID based on the current time
    /// and internal state. If successful, it returns [`Poll::Ready`] with a
    /// newly generated ID. If the generator is temporarily exhausted, it
    /// returns [`Poll::Pending`]. If an internal failure occurs (e.g., a lock
    /// error), it returns an error.
    ///
    /// # Returns
    /// - `Ok(Poll::Ready { id })`: A new ID is available
    /// - `Ok(Poll::Pending { yield_for })`: The suggested [`Duration`]
    ///   before trying again
    /// - `Err(e)`: the lock was poisoned
    ///
    /// Pending delays are [`Duration`] values with clock granularity already
    /// applied. [`Duration::ZERO`] permits an immediate retry.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "thread-rng")]
    /// # {
    /// use ferroid::{
    ///     generator::{LockMonoUlidGenerator, Poll},
    ///     id::ULID,
    ///     rand::ThreadRandom,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = LockMonoUlidGenerator::new(CachedClock::default(), ThreadRandom::default());
    ///
    /// // Attempt to generate a new ID
    /// let id: ULID = loop {
    ///     match generator.try_poll_id() {
    ///         Ok(Poll::Ready { id }) => break id,
    ///         Ok(Poll::Pending { yield_for }) => {
    ///             std::thread::sleep(yield_for);
    ///         }
    ///         Err(e) => panic!("Generator error: {}", e),
    ///     }
    /// };
    /// # }
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the generator fails, such as from lock poisoning.
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_poll_id(&self) -> Result<Poll<ID>, LockError> {
        let now = self.time.current_millis();

        let mut id = cfg_select! {{
            feature = "parking-lot" => { self.state.lock() }
            _ => { self.state.lock()? }
        }};

        let current_ts = id.timestamp();

        match now.cmp(&current_ts) {
            Ordering::Equal => {
                if id.has_random_room() {
                    *id = id.increment_random();
                    Ok(Poll::Ready { id: *id })
                } else {
                    Ok(Poll::Pending {
                        yield_for: Duration::from_millis(T::GRANULARITY_MILLIS.get()),
                    })
                }
            }
            Ordering::Greater => {
                let rand = self.rng.rand::<ID>();
                *id = id.rollover_to_timestamp(now, rand);
                Ok(Poll::Ready { id: *id })
            }
            Ordering::Less => Ok(Self::cold_clock_behind(now, current_ts)),
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

impl<ID, T, R> UlidGenerator<ID, T, R> for LockMonoUlidGenerator<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    type Err = LockError;

    fn new(time: T, rng: R) -> Self {
        Self::new(time, rng)
    }

    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err> {
        self.try_poll_id()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{CountingRand, MockTime, OverflowClock, TestClock, ZeroRand};
    use super::*;
    use crate::generator::Poll;
    use crate::id::ULID;
    use core::time::Duration;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn pending_delays_saturate_at_u64_milliseconds() {
        let lock =
            LockMonoUlidGenerator::<ULID, _, _>::from_components(2, 0, OverflowClock, ZeroRand);
        assert_eq!(
            lock.try_poll_id().unwrap(),
            Poll::Pending {
                yield_for: Duration::from_millis(u64::MAX)
            }
        );
    }

    #[test]
    fn handles_rollback_and_rollover() {
        let shared_time = TestClock::new(2);
        let generator: LockMonoUlidGenerator<ULID, _, _> =
            LockMonoUlidGenerator::from_components(2, ULID::max_random(), &shared_time, ZeroRand);
        // Exhaustion waits for the next tick: one tick is 10 milliseconds.
        assert_eq!(
            generator.try_poll_id().unwrap(),
            Poll::Pending {
                yield_for: Duration::from_millis(10)
            }
        );

        // Rolling back to zero puts the clock two ticks behind the stored timestamp.
        shared_time.ticks.set(0);
        assert_eq!(
            generator.try_poll_id().unwrap(),
            Poll::Pending {
                yield_for: Duration::from_millis(20)
            }
        );
        let id = generator
            .try_next_id(|delay| {
                assert_eq!(delay, Duration::from_millis(20));
                shared_time.ticks.set(3);
            })
            .unwrap();
        assert_eq!((id.timestamp(), id.random()), (3, 0));
    }

    #[test]
    fn is_monotonic_across_ticks() {
        let shared_time = TestClock::new(1);
        let generator: LockMonoUlidGenerator<ULID, _, _> =
            LockMonoUlidGenerator::new(&shared_time, CountingRand::new());
        // Two IDs in one tick increment the random field.
        let first = generator
            .try_next_id(|_| panic!("expected ready ID"))
            .unwrap();
        let second = generator
            .try_next_id(|_| panic!("expected ready ID"))
            .unwrap();
        assert_eq!((first.timestamp(), first.random()), (1, 0));
        assert_eq!((second.timestamp(), second.random()), (1, 1));
        assert!(second > first);

        shared_time.ticks.set(2);
        let next = generator
            .try_next_id(|_| panic!("expected ready ID"))
            .unwrap();
        assert_eq!((next.timestamp(), next.random()), (2, 1));
        assert!(next > second);
    }

    #[test]
    fn concurrent_uniqueness() {
        use std::{sync::Barrier, thread};

        use alloc::vec::Vec;

        const THREADS: usize = 8;
        const IDS_PER_THREAD: usize = 64;

        let generator = LockMonoUlidGenerator::<ULID, _, _>::new(MockTime { millis: 1 }, ZeroRand);
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

    #[cfg(not(feature = "parking-lot"))]
    #[test]
    fn is_poisoned_on_panic_std_mutex() {
        let generator: LockMonoUlidGenerator<ULID, _, _> =
            LockMonoUlidGenerator::new(MockTime { millis: 1 }, ZeroRand);

        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _guard = generator.state.lock();
            panic!("boom: poison the mutex");
        }));
        assert!(panic.is_err());

        let err = generator
            .try_poll_id()
            .expect_err("expected an error after poison");
        assert!(matches!(err, crate::generator::Error::LockPoisoned));
        let err = generator
            .try_next_id(|_| panic!("poisoned generator invoked backoff"))
            .expect_err("expected an error after poison");
        assert!(matches!(err, crate::generator::Error::LockPoisoned));
    }

    #[cfg(feature = "parking-lot")]
    #[test]
    fn is_not_poisoned_on_panic_parking_lot_mutex() {
        let generator: LockMonoUlidGenerator<ULID, _, _> =
            LockMonoUlidGenerator::new(MockTime { millis: 1 }, ZeroRand);

        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _guard = generator.state.lock();
            panic!("boom: poison the mutex");
        }));
        assert!(panic.is_err());

        generator
            .try_poll_id()
            .expect("parking_lot::Mutex cannot be poisoned");
    }
}
