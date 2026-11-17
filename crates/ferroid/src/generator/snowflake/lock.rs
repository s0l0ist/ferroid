use alloc::sync::Arc;
use core::{cmp::Ordering, convert::Infallible, time::Duration};

#[cfg(feature = "tracing")]
use tracing::instrument;

use crate::{
    generator::{LockError, Mutex, Poll, Result, SnowflakeGenerator},
    id::{SnowflakeId, ToU64},
    time::TimeSource,
};

/// A lock-based Snowflake ID generator suitable for multi-threaded
/// environments.
///
/// This generator wraps the Snowflake state in an [`Arc<Mutex<_>>`], allowing
/// safe shared use across threads.
///
/// ## Features
/// - ✅ Thread-safe
/// - ✅ Safely implement any [`SnowflakeId`] layout
///
/// ## Recommended When
/// - You're in a multi-threaded environment
/// - Fair access across threads is important
/// - Your target doesn't support atomics.
///
/// ## See Also
/// - [`BasicSnowflakeGenerator`]
/// - [`AtomicSnowflakeGenerator`]
///
/// [`BasicSnowflakeGenerator`]: crate::generator::BasicSnowflakeGenerator
/// [`AtomicSnowflakeGenerator`]: crate::generator::AtomicSnowflakeGenerator
pub struct LockSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    pub(crate) state: Arc<crossbeam_utils::CachePadded<Mutex<ID>>>,
    pub(crate) time: T,
}

impl<ID, T> LockSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    /// Creates a new [`LockSnowflakeGenerator`] initialized with the current
    /// time and a given machine ID.
    ///
    /// This constructor sets the initial timestamp and sequence to zero, and
    /// uses the provided `time` to fetch the current time during ID generation.
    /// It is the recommended way to create a new lock-based generator for
    /// typical use cases.
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
    /// A new [`LockSnowflakeGenerator`] ready to produce unique, time-ordered
    /// IDs.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "parking-lot")] {
    ///     use ferroid::{
    ///         generator::{Poll, LockSnowflakeGenerator},
    ///         id::SnowflakeTwitterId,
    ///         time::{CachedClock, TWITTER_EPOCH},
    ///     };
    ///
    ///     let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
    ///     let generator = LockSnowflakeGenerator::new(0, clock);
    ///
    ///     let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// }
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
            state: Arc::new(crossbeam_utils::CachePadded::new(Mutex::new(id))),
            time,
        }
    }

    /// Generates a new ID.
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
    /// # #[cfg(feature = "parking-lot")] {
    /// use ferroid::{
    ///     generator::{LockSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = LockSnowflakeGenerator::new(0, CachedClock::default());
    ///
    /// let id: SnowflakeTwitterId = generator.next_id(|_| std::thread::yield_now());
    /// # }
    /// ```
    pub fn next_id(&self, f: impl FnMut(Duration)) -> ID
    where
        Self: SnowflakeGenerator<ID, T, Err: Into<Infallible>>,
    {
        SnowflakeGenerator::next_id(self, f)
    }

    /// Generates a new ID.
    ///
    /// Returns a new, time-ordered, unique ID with fallible error handling.
    ///
    /// When generation is pending, `f` receives a suggested [`Duration`],
    /// with clock granularity already applied. The delay may be zero and
    /// saturates at `u64::MAX` milliseconds.
    ///
    /// # Example
    /// ```
    /// use ferroid::{
    ///     generator::{LockSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::CachedClock,
    /// };
    ///
    /// let generator = LockSnowflakeGenerator::new(0, CachedClock::default());
    ///
    /// let id: SnowflakeTwitterId = match generator.try_next_id(|_| std::thread::yield_now()) {
    ///     Ok(id) => id,
    ///     Err(_) => unreachable!(),
    /// };
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the generator fails, such as from lock poisoning.
    pub fn try_next_id(&self, f: impl FnMut(Duration)) -> Result<ID, LockError> {
        SnowflakeGenerator::try_next_id(self, f)
    }

    /// Attempts to generate the next available ID.
    ///
    /// Returns a new, time-ordered, unique ID if generation succeeds. If the
    /// generator is temporarily exhausted (e.g., the sequence is full and the
    /// time has not advanced), it returns [`Poll::Pending`].
    ///
    /// Requires an infallible mutex backend (`parking-lot`).
    ///
    /// Pending delays are [`Duration`] values with clock granularity already
    /// applied. [`Duration::ZERO`] permits an immediate retry.
    ///
    /// # Example
    /// ```
    /// # #[cfg(feature = "parking-lot")] {
    /// use ferroid::{
    ///     generator::{LockSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{CachedClock, TWITTER_EPOCH},
    /// };
    ///
    /// let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
    /// let generator = LockSnowflakeGenerator::new(0, clock);
    ///
    /// let id: SnowflakeTwitterId = loop {
    ///     match generator.poll_id() {
    ///         Poll::Ready { id } => break id,
    ///         Poll::Pending { .. } => std::thread::yield_now(),
    ///     }
    /// };
    /// # }
    /// ```
    pub fn poll_id(&self) -> Poll<ID>
    where
        Self: SnowflakeGenerator<ID, T, Err: Into<Infallible>>,
    {
        SnowflakeGenerator::poll_id(self)
    }

    /// Attempts to generate a new Snowflake ID with fallible error handling.
    ///
    /// This method attempts to generate the next ID based on the current time
    /// and internal state. If successful, it returns [`Poll::Ready`] with a
    /// newly generated ID. If the generator is temporarily exhausted, it
    /// returns [`Poll::Pending`]. If an internal failure occurs (e.g., a time
    /// source or lock error), it returns an error.
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
    /// use ferroid::{
    ///     generator::{LockSnowflakeGenerator, Poll},
    ///     id::SnowflakeTwitterId,
    ///     time::{CachedClock, TWITTER_EPOCH},
    /// };
    ///
    /// let clock: CachedClock = CachedClock::with_epoch(TWITTER_EPOCH);
    /// let generator = LockSnowflakeGenerator::new(0, clock);
    ///
    /// // Attempt to generate a new ID
    /// let id: SnowflakeTwitterId = loop {
    ///     match generator.try_poll_id() {
    ///         Ok(Poll::Ready { id }) => break id,
    ///         Ok(Poll::Pending { yield_for }) => {
    ///             std::thread::sleep(yield_for);
    ///         }
    ///         Err(e) => panic!("Generator error: {}", e),
    ///     }
    /// };
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the generator fails, such as from lock poisoning.
    #[cfg_attr(feature = "tracing", instrument(level = "trace", skip(self)))]
    pub fn try_poll_id(&self) -> Result<Poll<ID>, LockError> {
        let now = self.time.current_millis();

        let mut id = core::cfg_select! {
            feature = "parking-lot" => self.state.lock(),
            _ => self.state.lock()?,
        };

        let current_ts = id.timestamp();
        match now.cmp(&current_ts) {
            Ordering::Equal => {
                if id.has_sequence_room() {
                    *id = id.increment_sequence();
                    Ok(Poll::Ready { id: *id })
                } else {
                    Ok(Poll::Pending {
                        yield_for: Duration::from_millis(T::GRANULARITY_MILLIS.get()),
                    })
                }
            }
            Ordering::Greater => {
                *id = id.rollover_to_timestamp(now);
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

impl<ID, T> SnowflakeGenerator<ID, T> for LockSnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err = LockError;

    fn new(machine_id: ID::Ty, time: T) -> Self {
        Self::new(machine_id, time)
    }

    fn try_poll_id(&self) -> Result<Poll<ID>, Self::Err> {
        self.try_poll_id()
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
    use std::panic::{AssertUnwindSafe, catch_unwind};

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

            {
                let generator = LockSnowflakeGenerator::<SmallId, _>::from_components(
                    timestamp, 0, 0, SlowClock,
                );
                assert_eq!(
                    generator.try_poll_id().unwrap(),
                    Poll::Pending {
                        yield_for: expected
                    }
                );
            }
        }
    }

    #[test]
    fn pending_delays_saturate_at_u64_milliseconds() {
        let lock = LockSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(
            2,
            0,
            0,
            OverflowClock,
        );
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
        let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
            LockSnowflakeGenerator::from_components(
                2,
                1,
                SnowflakeTwitterId::max_sequence(),
                &shared_time,
            );
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
        assert_eq!((id.timestamp(), id.sequence()), (3, 0));
    }

    #[test]
    fn is_monotonic_across_ticks() {
        let shared_time = TestClock::new(1);
        let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
            LockSnowflakeGenerator::new(1, &shared_time);
        // Two IDs in one tick increment the sequence.
        let first = generator
            .try_next_id(|_| panic!("expected ready ID"))
            .unwrap();
        let second = generator
            .try_next_id(|_| panic!("expected ready ID"))
            .unwrap();
        assert_eq!((first.timestamp(), first.sequence()), (1, 0));
        assert_eq!((second.timestamp(), second.sequence()), (1, 1));
        assert!(second > first);

        shared_time.ticks.set(2);
        let next = generator
            .try_next_id(|_| panic!("expected ready ID"))
            .unwrap();
        assert_eq!((next.timestamp(), next.sequence()), (2, 0));
        assert!(next > second);
    }

    #[test]
    fn concurrent_uniqueness() {
        use std::{sync::Barrier, thread};

        use alloc::vec::Vec;

        const THREADS: usize = 8;
        // The whole batch fits within one Snowflake timestamp's sequence range.
        const IDS_PER_THREAD: usize = 64;

        let generator =
            LockSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, MockTime { millis: 1 });
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

    #[cfg(not(feature = "parking-lot"))]
    #[test]
    fn is_poisoned_on_panic_std_mutex() {
        let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
            LockSnowflakeGenerator::new(0u64, MockTime { millis: 1 });

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
        let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
            LockSnowflakeGenerator::new(0u64, MockTime { millis: 1 });

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
