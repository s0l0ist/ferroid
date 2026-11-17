use alloc::{rc::Rc, vec, vec::Vec};
use core::{cell::Cell, num::NonZeroU64, time::Duration};
#[cfg(feature = "lock")]
use std::panic::{AssertUnwindSafe, catch_unwind};
#[cfg(all(feature = "std", any(feature = "lock", feature = "atomic")))]
use std::{sync::Barrier, thread};

#[cfg(feature = "lock")]
use crate::generator::LockMonoUlidGenerator;
#[cfg(feature = "basic")]
use crate::generator::{BasicMonoUlidGenerator, BasicUlidGenerator};
use crate::{
    generator::{Poll, UlidGenerator},
    id::{Id, ToU64, ULID, UlidId},
    rand::RandSource,
    time::TimeSource,
};

struct MockTime {
    millis: u128,
}
impl TimeSource<u128> for MockTime {
    fn current_millis(&self) -> u128 {
        self.millis
    }
}

struct MockRand {
    rand: u128,
}

impl RandSource<u128> for MockRand {
    fn rand(&self) -> u128 {
        self.rand
    }
}

#[derive(Clone)]
struct SharedMockStepTime {
    clock: Rc<MockStepTime>,
}

impl SharedMockStepTime {
    fn new(values: Vec<u64>, index: usize) -> Self {
        Self {
            clock: Rc::new(MockStepTime {
                values,
                index: Cell::new(index),
            }),
        }
    }
}

impl TimeSource<u128> for SharedMockStepTime {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(10).unwrap();

    fn current_millis(&self) -> u128 {
        u128::from(self.clock.values[self.clock.index.get()])
    }
}
struct MockStepTime {
    values: Vec<u64>,
    index: Cell<usize>,
}

struct FixedTime;
impl TimeSource<u128> for FixedTime {
    fn current_millis(&self) -> u128 {
        0
    }
}

struct MinRand;
impl RandSource<u128> for MinRand {
    fn rand(&self) -> u128 {
        0
    }
}

struct CoarseTime;
impl TimeSource<u128> for CoarseTime {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::MAX;

    fn current_millis(&self) -> u128 {
        0
    }
}

#[test]
fn pending_delays_saturate_at_u64_milliseconds() {
    #[cfg(feature = "basic")]
    let basic = BasicMonoUlidGenerator::<ULID, _, _>::from_components(2, 0, CoarseTime, MinRand);
    #[cfg(feature = "basic")]
    assert_eq!(
        basic.poll_id().unwrap_pending(),
        Duration::from_millis(u64::MAX)
    );

    #[cfg(feature = "lock")]
    let lock = LockMonoUlidGenerator::<ULID, _, _>::from_components(2, 0, CoarseTime, MinRand);
    #[cfg(feature = "lock")]
    assert_eq!(
        lock.try_poll_id().unwrap().unwrap_pending(),
        Duration::from_millis(u64::MAX)
    );

    #[cfg(feature = "atomic")]
    let atomic = crate::generator::AtomicMonoUlidGenerator::<ULID, _, _>::from_components(
        2, 0, CoarseTime, MinRand,
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
    crate::define_ulid!(WideTimestampId, u128, reserved: 0, timestamp: 128, random: 0);
    let timestamp = u128::from(u64::MAX) + 1;
    let generator = BasicMonoUlidGenerator::<WideTimestampId, _, _>::from_components(
        timestamp,
        0,
        MockTime { millis: 0 },
        MinRand,
    );
    assert_eq!(
        generator.poll_id().unwrap_pending(),
        Duration::from_millis(u64::MAX)
    );

    let overflowing = BasicMonoUlidGenerator::<WideTimestampId, _, _>::from_components(
        u128::MAX,
        0,
        CoarseTime,
        MinRand,
    );
    assert_eq!(
        overflowing.poll_id().unwrap_pending(),
        Duration::from_millis(u64::MAX)
    );
}

struct MaxRand;
impl RandSource<u128> for MaxRand {
    fn rand(&self) -> u128 {
        u128::MAX
    }
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

fn run_id_sequence_increments_within_same_tick<G, ID, T, R>(generator: &G)
where
    G: UlidGenerator<ID, T, R>,
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let id1 = generator.try_poll_id().unwrap().unwrap_ready();
    let id2 = generator.try_poll_id().unwrap().unwrap_ready();
    let id3 = generator.try_poll_id().unwrap().unwrap_ready();

    assert_eq!(id1.timestamp().to_u64(), 42);
    assert_eq!(id2.timestamp().to_u64(), 42);
    assert_eq!(id3.timestamp().to_u64(), 42);
    assert_eq!(id1.random().to_u64(), 42);
    assert_eq!(id2.random().to_u64(), 42 + 1);
    assert_eq!(id3.random().to_u64(), 42 + 2);
    assert!(id1 < id2 && id2 < id3);
}

fn run_generator_returns_pending_when_sequence_exhausted<G, ID, T, R>(generator: &G)
where
    G: UlidGenerator<ID, T, R>,
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let yield_for = generator.try_poll_id().unwrap().unwrap_pending();
    assert_eq!(
        yield_for,
        Duration::from_millis(T::GRANULARITY_MILLIS.get())
    );
}

fn run_generator_handles_rollover<G, ID, T, R>(generator: &G, shared_time: &SharedMockStepTime)
where
    G: UlidGenerator<ID, T, R>,
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let id = generator.try_poll_id().unwrap().unwrap_ready();
    assert_eq!(id.timestamp().to_u64(), 42);

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
}

fn run_generator_monotonic<G, ID, T, R>(generator: &G, shared_time: &SharedMockStepTime)
where
    G: UlidGenerator<ID, T, R>,
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut previous = None;
    for (index, &timestamp) in shared_time.clock.values.iter().enumerate() {
        shared_time.clock.index.set(index);
        for random in 42..45 {
            let id = generator.try_poll_id().unwrap().unwrap_ready();
            assert_eq!(id.timestamp().to_u64(), timestamp);
            assert_eq!(id.random().to_u64(), random);
            if let Some(previous) = previous {
                assert!(id > previous);
            }
            previous = Some(id);
        }
    }
}

#[cfg(all(feature = "std", any(feature = "lock", feature = "atomic")))]
fn run_generator_concurrent_uniqueness<G, ID, T, R>(make_generator: impl Fn() -> G)
where
    G: UlidGenerator<ID, T, R> + Send + Sync,
    ID: UlidId + Send,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    const THREADS: usize = 8;
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
    for (random, id) in ids.iter().enumerate() {
        assert_eq!(id.timestamp().to_u64(), 42);
        assert_eq!(id.random().to_u64(), u64::try_from(random).unwrap());
    }
}

#[cfg(feature = "basic")]
#[test]
fn basic_generator() {
    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 43 };
    let generator: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(mock_time, mock_rand);
    let id = generator.poll_id().unwrap_ready();
    assert_eq!(id.timestamp(), 42);
    assert_eq!(id.random(), 43);
}

#[cfg(feature = "basic")]
#[test]
fn basic_generator_mono_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 42 };
    let generator: BasicMonoUlidGenerator<ULID, _, _> =
        BasicMonoUlidGenerator::new(mock_time, mock_rand);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_mono_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 42 };

    let generator: LockMonoUlidGenerator<ULID, _, _> =
        LockMonoUlidGenerator::new(mock_time, mock_rand);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_generator_mono_sequence_test() {
    use crate::generator::AtomicMonoUlidGenerator;

    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 42 };

    let generator: AtomicMonoUlidGenerator<ULID, _, _> =
        AtomicMonoUlidGenerator::new(mock_time, mock_rand);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[cfg(feature = "basic")]
#[test]
fn basic_generator_mono_pending_test() {
    let generator: BasicMonoUlidGenerator<ULID, _, _> =
        BasicMonoUlidGenerator::from_components(0, ULID::max_random(), FixedTime, MinRand);
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_mono_pending_test() {
    let generator: LockMonoUlidGenerator<ULID, _, _> =
        LockMonoUlidGenerator::from_components(0, ULID::max_random(), FixedTime, MinRand);
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_generator_mono_pending_test() {
    use crate::generator::AtomicMonoUlidGenerator;

    let generator: AtomicMonoUlidGenerator<ULID, _, _> =
        AtomicMonoUlidGenerator::from_components(0, ULID::max_random(), FixedTime, MinRand);
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[cfg(feature = "basic")]
#[test]
fn basic_generator_mono_rollover_test() {
    let shared_time = SharedMockStepTime::new(vec![42, 39, 42, 43], 0);
    let generator: BasicMonoUlidGenerator<ULID, _, _> =
        BasicMonoUlidGenerator::new(shared_time.clone(), MaxRand);
    run_generator_handles_rollover(&generator, &shared_time);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_mono_rollover_test() {
    let shared_time = SharedMockStepTime::new(vec![42, 39, 42, 43], 0);
    let generator: LockMonoUlidGenerator<ULID, _, _> =
        LockMonoUlidGenerator::new(shared_time.clone(), MaxRand);
    run_generator_handles_rollover(&generator, &shared_time);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_generator_mono_rollover_test() {
    use crate::generator::AtomicMonoUlidGenerator;

    let shared_time = SharedMockStepTime::new(vec![42, 39, 42, 43], 0);
    let generator: AtomicMonoUlidGenerator<ULID, _, _> =
        AtomicMonoUlidGenerator::new(shared_time.clone(), MaxRand);
    run_generator_handles_rollover(&generator, &shared_time);
}

#[cfg(feature = "basic")]
#[test]
fn basic_generator_is_monotonic_across_ticks() {
    let shared_time = SharedMockStepTime::new(vec![42, 43, 44], 0);
    let generator: BasicMonoUlidGenerator<ULID, _, _> =
        BasicMonoUlidGenerator::new(shared_time.clone(), MockRand { rand: 42 });
    run_generator_monotonic(&generator, &shared_time);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_is_monotonic_across_ticks() {
    let shared_time = SharedMockStepTime::new(vec![42, 43, 44], 0);
    let generator: LockMonoUlidGenerator<ULID, _, _> =
        LockMonoUlidGenerator::new(shared_time.clone(), MockRand { rand: 42 });
    run_generator_monotonic(&generator, &shared_time);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_generator_is_monotonic_across_ticks() {
    use crate::generator::AtomicMonoUlidGenerator;

    let shared_time = SharedMockStepTime::new(vec![42, 43, 44], 0);
    let generator: AtomicMonoUlidGenerator<ULID, _, _> =
        AtomicMonoUlidGenerator::new(shared_time.clone(), MockRand { rand: 42 });
    run_generator_monotonic(&generator, &shared_time);
}

#[cfg(feature = "lock")]
#[test]
fn lock_generator_concurrent_uniqueness() {
    run_generator_concurrent_uniqueness(|| {
        LockMonoUlidGenerator::<ULID, _, _>::new(MockTime { millis: 42 }, MinRand)
    });
}

#[cfg(all(feature = "atomic", feature = "std"))]
#[test]
fn atomic_generator_concurrent_uniqueness() {
    use crate::generator::AtomicMonoUlidGenerator;

    run_generator_concurrent_uniqueness(|| {
        AtomicMonoUlidGenerator::<ULID, _, _>::new(MockTime { millis: 42 }, MinRand)
    });
}

#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
#[test]
fn lock_is_poisoned_on_panic_std_mutex() {
    let generator: LockMonoUlidGenerator<ULID, _, _> =
        LockMonoUlidGenerator::new(MockTime { millis: 42 }, MinRand);

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
    let generator: LockMonoUlidGenerator<ULID, _, _> =
        LockMonoUlidGenerator::new(MockTime { millis: 42 }, MinRand);

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
    let generator = BasicUlidGenerator::new(MockTime { millis: 42 }, MinRand);
    let backoff = |_: Duration| panic!("expected ready ID");
    let _id: ULID = generator.next_id(backoff);
    let _id: ULID = UlidGenerator::next_id(&generator, backoff);
}

#[cfg(feature = "basic")]
#[test]
fn basic_mono_can_call_next_id() {
    let generator = BasicMonoUlidGenerator::new(MockTime { millis: 42 }, MinRand);
    let backoff = |_: Duration| panic!("expected ready ID");
    let _id: ULID = generator.next_id(backoff);
    let _id: ULID = UlidGenerator::next_id(&generator, backoff);
}

#[cfg(feature = "atomic")]
#[test]
fn atomic_mono_can_call_next_id() {
    use crate::generator::AtomicMonoUlidGenerator;
    let generator = AtomicMonoUlidGenerator::new(MockTime { millis: 42 }, MinRand);
    let backoff = |_: Duration| panic!("expected ready ID");
    let _id: ULID = generator.next_id(backoff);
    let _id: ULID = UlidGenerator::next_id(&generator, backoff);
}

#[cfg(feature = "parking-lot")]
#[test]
fn lock_mono_can_call_next_id() {
    let generator = LockMonoUlidGenerator::new(MockTime { millis: 42 }, MinRand);
    let backoff = |_: Duration| panic!("expected ready ID");
    let _id: ULID = generator.next_id(backoff);
    let _id: ULID = UlidGenerator::next_id(&generator, backoff);
}

#[cfg(feature = "lock")]
#[test]
fn lock_mono_can_call_try_next_id() {
    let generator = LockMonoUlidGenerator::new(MockTime { millis: 42 }, MinRand);
    let backoff = |_: Duration| panic!("expected ready ID");
    let first: ULID = generator.try_next_id(backoff).unwrap();
    let second: ULID = UlidGenerator::try_next_id(&generator, backoff).unwrap();
    assert!(second > first);
}
