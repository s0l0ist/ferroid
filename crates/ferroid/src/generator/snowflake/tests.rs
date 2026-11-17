use alloc::{rc::Rc, vec, vec::Vec};
use core::{cell::Cell, num::NonZeroU64, time::Duration};
#[cfg(feature = "lock")]
use std::panic::{AssertUnwindSafe, catch_unwind};
#[cfg(all(feature = "std", any(feature = "lock", feature = "atomic")))]
use std::{sync::Barrier, thread};

#[cfg(feature = "basic")]
use crate::generator::BasicSnowflakeGenerator;
#[cfg(feature = "lock")]
use crate::generator::LockSnowflakeGenerator;
use crate::{
    generator::{Poll, SnowflakeGenerator},
    id::{Id, SnowflakeId, SnowflakeTwitterId, ToU64},
    time::TimeSource,
};

struct MockTime {
    millis: u64,
}

impl TimeSource<u64> for MockTime {
    fn current_millis(&self) -> u64 {
        self.millis
    }
}

impl TimeSource<u128> for MockTime {
    fn current_millis(&self) -> u128 {
        u128::from(<Self as TimeSource<u64>>::current_millis(self))
    }
}

#[derive(Clone)]
struct SharedMockStepTime {
    clock: Rc<MockStepTime>,
}

impl TimeSource<u64> for SharedMockStepTime {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(10).unwrap();

    fn current_millis(&self) -> u64 {
        self.clock.values[self.clock.index.get()]
    }
}
struct MockStepTime {
    values: Vec<u64>,
    index: Cell<usize>,
}

struct FixedTime;
impl TimeSource<u64> for FixedTime {
    fn current_millis(&self) -> u64 {
        0
    }
}

struct CoarseTime;
impl TimeSource<u64> for CoarseTime {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::MAX;

    fn current_millis(&self) -> u64 {
        0
    }
}

#[cfg(any(feature = "basic", feature = "lock"))]
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
        const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(5_000_000_000).unwrap();

        fn current_millis(&self) -> u32 {
            0
        }
    }

    for (timestamp, expected) in [
        (0, Duration::from_secs(5_000_000)),
        (3, Duration::from_mins(250_000)),
    ] {
        #[cfg(feature = "basic")]
        {
            let generator =
                BasicSnowflakeGenerator::<SmallId, _>::from_components(timestamp, 0, 0, SlowClock);
            assert_eq!(generator.poll_id().unwrap_pending(), expected);
        }
        #[cfg(feature = "lock")]
        {
            let generator =
                LockSnowflakeGenerator::<SmallId, _>::from_components(timestamp, 0, 0, SlowClock);
            assert_eq!(generator.try_poll_id().unwrap().unwrap_pending(), expected);
        }
    }
}

#[test]
fn pending_delays_saturate_at_u64_milliseconds() {
    #[cfg(feature = "basic")]
    let basic =
        BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(2, 0, 0, CoarseTime);
    #[cfg(feature = "basic")]
    assert_eq!(
        basic.poll_id().unwrap_pending(),
        Duration::from_millis(u64::MAX)
    );

    #[cfg(feature = "lock")]
    let lock =
        LockSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(2, 0, 0, CoarseTime);
    #[cfg(feature = "lock")]
    assert_eq!(
        lock.try_poll_id().unwrap().unwrap_pending(),
        Duration::from_millis(u64::MAX)
    );

    #[cfg(feature = "atomic")]
    let atomic =
        crate::generator::AtomicSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(
            2, 0, 0, CoarseTime,
        );
    #[cfg(feature = "atomic")]
    assert_eq!(
        atomic.poll_id().unwrap_pending(),
        Duration::from_millis(u64::MAX)
    );
}

#[cfg(feature = "basic")]
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
        generator.poll_id().unwrap_pending(),
        Duration::from_millis(u64::MAX)
    );
}

trait IdGenStatusExt<ID>
where
    ID: Id,
{
    fn unwrap_ready(self) -> ID;
    fn unwrap_pending(self) -> Duration;
}

impl<ID> IdGenStatusExt<ID> for Poll<ID>
where
    ID: Id,
{
    fn unwrap_ready(self) -> ID {
        match self {
            Self::Ready { id } => id,
            Self::Pending { yield_for } => {
                panic!("unexpected pending (yield for: {yield_for:?})")
            }
        }
    }

    fn unwrap_pending(self) -> Duration {
        match self {
            Self::Ready { id } => panic!("unexpected ready ({id:?})"),
            Self::Pending { yield_for } => yield_for,
        }
    }
}

fn run_id_sequence_increments_within_same_tick<G, ID, T>(generator: &G)
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    let id1 = generator.try_poll_id().unwrap().unwrap_ready();
    let id2 = generator.try_poll_id().unwrap().unwrap_ready();
    let id3 = generator.try_poll_id().unwrap().unwrap_ready();

    assert_eq!(id1.timestamp().to_u64(), 42);
    assert_eq!(id2.timestamp().to_u64(), 42);
    assert_eq!(id3.timestamp().to_u64(), 42);
    assert_eq!(id1.sequence().to_u64(), 0);
    assert_eq!(id2.sequence().to_u64(), 1);
    assert_eq!(id3.sequence().to_u64(), 2);
    assert!(id1 < id2 && id2 < id3);
}

fn run_generator_returns_pending_when_sequence_exhausted<G, ID, T>(generator: &G)
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    let yield_for = generator.try_poll_id().unwrap().unwrap_pending();
    assert_eq!(
        yield_for,
        Duration::from_millis(T::GRANULARITY_MILLIS.get())
    );
}

fn run_generator_handles_rollover<G, ID, T>(generator: &G, shared_time: &SharedMockStepTime)
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    for i in 0..=ID::max_sequence().to_u64() {
        let id = generator.try_poll_id().unwrap().unwrap_ready();
        assert_eq!(id.sequence().to_u64(), i);
        assert_eq!(id.timestamp().to_u64(), 42);
    }

    let yield_for = generator.try_poll_id().unwrap().unwrap_pending();
    assert_eq!(yield_for, Duration::from_millis(10));

    // Rolling back the clock must leave the exhausted generator unchanged.
    shared_time.clock.index.set(1);
    for _ in 0..2 {
        assert_eq!(
            generator.try_poll_id().unwrap().unwrap_pending(),
            Duration::from_millis(30)
        );
    }

    shared_time.clock.index.set(2);
    assert_eq!(
        generator.try_poll_id().unwrap().unwrap_pending(),
        Duration::from_millis(10),
    );

    // Callback durations include clock granularity exactly once.
    let mut delays = Vec::new();
    let id = generator
        .try_next_id(|delay| {
            delays.push(delay);
            shared_time.clock.index.set(match delays.len() {
                1 => 1, // Roll the clock back three ticks.
                2 => 3, // Advance past the exhausted timestamp.
                _ => panic!("unexpected retry"),
            });
        })
        .unwrap();
    assert_eq!(
        delays,
        [Duration::from_millis(10), Duration::from_millis(30)]
    );
    assert_eq!(id.timestamp().to_u64(), 43);
    assert_eq!(id.sequence().to_u64(), 0);
}

fn run_generator_monotonic<G, ID, T>(generator: &G, shared_time: &SharedMockStepTime)
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    let mut previous = None;
    for (index, &timestamp) in shared_time.clock.values.iter().enumerate() {
        shared_time.clock.index.set(index);
        for sequence in 0..3 {
            let id = generator.try_poll_id().unwrap().unwrap_ready();
            assert_eq!(id.timestamp().to_u64(), timestamp);
            assert_eq!(id.machine_id(), ID::ONE);
            assert_eq!(id.sequence().to_u64(), sequence);
            if let Some(previous) = previous {
                assert!(id > previous);
            }
            previous = Some(id);
        }
    }
}

#[cfg(all(feature = "std", any(feature = "lock", feature = "atomic")))]
fn run_generator_concurrent_uniqueness<G, ID, T>(make_generator: impl Fn() -> G)
where
    G: SnowflakeGenerator<ID, T> + Send + Sync,
    ID: SnowflakeId + Send,
    T: TimeSource<ID::Ty>,
{
    const THREADS: usize = 8;
    // The whole batch fits within one Snowflake timestamp's sequence range.
    const IDS_PER_THREAD: usize = 64;

    let generator = make_generator();
    let barrier = Barrier::new(THREADS);
    let mut ids = Vec::with_capacity(THREADS * IDS_PER_THREAD);

    thread::scope(|s| {
        let mut handles = Vec::with_capacity(THREADS);
        for _ in 0..THREADS {
            handles.push(s.spawn(|| {
                barrier.wait();
                let mut ids = Vec::with_capacity(IDS_PER_THREAD);
                let mut retries = 0;
                for _ in 0..IDS_PER_THREAD {
                    let id = generator
                        .try_next_id(|yield_for| {
                            // Only a lost CAS may be pending at this fixed timestamp.
                            assert_eq!(yield_for, Duration::ZERO);
                            retries += 1;
                            assert!(retries < THREADS * IDS_PER_THREAD, "generator stalled");
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
        assert_eq!(id.timestamp().to_u64(), 42);
        assert_eq!(id.machine_id(), ID::ZERO);
        assert_eq!(id.sequence().to_u64(), u64::try_from(sequence).unwrap());
    }
}

#[cfg(feature = "basic")]
#[test]
fn basic_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_generator_sequence_test() {
    use crate::generator::AtomicSnowflakeGenerator;

    let mock_time = MockTime { millis: 42 };
    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[cfg(feature = "basic")]
#[test]
fn basic_generator_pending_test() {
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::from_components(
            0,
            0,
            SnowflakeTwitterId::max_sequence(),
            FixedTime,
        );
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_pending_test() {
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::from_components(
            0,
            0,
            SnowflakeTwitterId::max_sequence(),
            FixedTime,
        );
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_generator_pending_test() {
    use crate::generator::AtomicSnowflakeGenerator;

    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::from_components(
            0,
            0,
            SnowflakeTwitterId::max_sequence(),
            FixedTime,
        );
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[cfg(feature = "basic")]
#[test]
fn basic_generator_rollover_test() {
    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 39, 42, 43],
            index: Cell::new(0),
        }),
    };
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(&generator, &shared_time);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_rollover_test() {
    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 39, 42, 43],
            index: Cell::new(0),
        }),
    };
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(&generator, &shared_time);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_generator_rollover_test() {
    use crate::generator::AtomicSnowflakeGenerator;

    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 39, 42, 43],
            index: Cell::new(0),
        }),
    };
    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(&generator, &shared_time);
}

#[cfg(feature = "basic")]
#[test]
fn basic_generator_is_monotonic_across_ticks() {
    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 43, 44],
            index: Cell::new(0),
        }),
    };
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_monotonic(&generator, &shared_time);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_is_monotonic_across_ticks() {
    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 43, 44],
            index: Cell::new(0),
        }),
    };
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_monotonic(&generator, &shared_time);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_generator_is_monotonic_across_ticks() {
    use crate::generator::AtomicSnowflakeGenerator;

    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 43, 44],
            index: Cell::new(0),
        }),
    };
    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_monotonic(&generator, &shared_time);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_concurrent_uniqueness() {
    run_generator_concurrent_uniqueness(|| {
        LockSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, MockTime { millis: 42 })
    });
}

#[cfg(all(feature = "atomic", feature = "std"))]
#[test]
fn atomic_generator_concurrent_uniqueness() {
    use crate::generator::AtomicSnowflakeGenerator;

    run_generator_concurrent_uniqueness(|| {
        AtomicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, MockTime { millis: 42 })
    });
}

#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
#[test]
fn lock_is_poisoned_on_panic_std_mutex() {
    // Arrange
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(0u64, MockTime { millis: 42 });

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
fn lock_is_not_poisoned_on_panic_parking_lot_mutex() {
    // Arrange
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(0u64, MockTime { millis: 42 });

    let panic = catch_unwind(AssertUnwindSafe(|| {
        let _guard = generator.state.lock();
        panic!("boom: poison the mutex");
    }));
    assert!(panic.is_err());

    generator
        .try_poll_id()
        .expect("parking_lot::Mutex cannot be poisoned");
}

#[cfg(feature = "basic")]
#[test]
fn basic_can_call_next_id() {
    let generator = BasicSnowflakeGenerator::new(0, MockTime { millis: 42 });
    let backoff = |_: Duration| panic!("expected ready ID");
    let _id: SnowflakeTwitterId = generator.next_id(backoff);
    let _id: SnowflakeTwitterId = SnowflakeGenerator::next_id(&generator, backoff);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_can_call_next_id() {
    use crate::generator::AtomicSnowflakeGenerator;

    let generator = AtomicSnowflakeGenerator::new(0, MockTime { millis: 42 });
    let backoff = |_: Duration| panic!("expected ready ID");
    let _id: SnowflakeTwitterId = generator.next_id(backoff);
    let _id: SnowflakeTwitterId = SnowflakeGenerator::next_id(&generator, backoff);
}

#[cfg(feature = "parking-lot")]
#[test]
fn lock_can_call_next_id() {
    let generator = LockSnowflakeGenerator::new(0, MockTime { millis: 42 });
    let backoff = |_: Duration| panic!("expected ready ID");
    let _id: SnowflakeTwitterId = generator.next_id(backoff);
    let _id: SnowflakeTwitterId = SnowflakeGenerator::next_id(&generator, backoff);
}

#[cfg(feature = "lock")]
#[test]
fn lock_can_call_try_next_id() {
    let generator = LockSnowflakeGenerator::new(0, MockTime { millis: 42 });
    let backoff = |_: Duration| panic!("expected ready ID");
    let first: SnowflakeTwitterId = generator.try_next_id(backoff).unwrap();
    let second: SnowflakeTwitterId = SnowflakeGenerator::try_next_id(&generator, backoff).unwrap();
    assert!(second > first);
}
