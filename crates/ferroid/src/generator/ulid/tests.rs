use alloc::{rc::Rc, sync::Arc, vec, vec::Vec};
use core::cell::Cell;
use std::{
    collections::HashSet,
    sync::Mutex,
    thread::{self, scope},
};

use crate::{
    generator::{
        BasicMonoUlidGenerator, BasicUlidGenerator, IdGenStatus, LockMonoUlidGenerator,
        UlidGenerator,
    },
    id::{Id, ToU64, ULID, UlidId},
    rand::{RandSource, ThreadRandom},
    time::{MonotonicClock, TimeSource},
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

struct MaxRand;
impl RandSource<u128> for MaxRand {
    fn rand(&self) -> u128 {
        u128::MAX
    }
}

trait IdGenStatusExt<T>
where
    T: Id,
{
    fn unwrap_ready(self) -> T;
    fn unwrap_pending(self) -> T::Ty;
}

impl<T> IdGenStatusExt<T> for IdGenStatus<T>
where
    T: Id,
{
    fn unwrap_ready(self) -> T {
        match self {
            Self::Ready { id } => id,
            Self::Pending { yield_for } => {
                panic!("unexpected pending (yield for: {yield_for})")
            }
        }
    }

    fn unwrap_pending(self) -> T::Ty {
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
    let id1 = generator.try_next_id().unwrap().unwrap_ready();
    let id2 = generator.try_next_id().unwrap().unwrap_ready();
    let id3 = generator.try_next_id().unwrap().unwrap_ready();

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
    let yield_for = generator.try_next_id().unwrap().unwrap_pending();
    assert_eq!(yield_for, ID::ONE);
}

fn run_generator_handles_rollover<G, ID, T, R>(generator: &G, shared_time: &SharedMockStepTime)
where
    G: UlidGenerator<ID, T, R>,
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let id = generator.try_next_id().unwrap().unwrap_ready();
    assert_eq!(id.timestamp().to_u64(), 42);

    let yield_for = generator.try_next_id().unwrap().unwrap_pending();
    assert_eq!(yield_for, ID::ONE);

    shared_time.clock.index.set(1);

    let id = generator.try_next_id().unwrap().unwrap_ready();
    assert_eq!(id.timestamp().to_u64(), 43);
}

fn run_generator_monotonic<G, ID, T, R>(generator: &G)
where
    G: UlidGenerator<ID, T, R>,
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut last_timestamp = ID::ZERO;
    let mut random = None;
    #[allow(clippy::items_after_statements)]
    const TOTAL_IDS: usize = 4096 * 256;

    for _ in 0..TOTAL_IDS {
        loop {
            match generator.try_next_id().unwrap() {
                IdGenStatus::Ready { id } => {
                    let ts = id.timestamp();
                    if ts > last_timestamp {
                        random = Some(id.random());
                    }

                    assert!(ts >= last_timestamp);
                    assert_eq!(id.random(), random.unwrap());

                    last_timestamp = ts;
                    random = random.map(|r| r + ID::ONE);
                    break;
                }
                IdGenStatus::Pending { .. } => {
                    core::hint::spin_loop();
                }
            }
        }
    }
}

fn run_generator_monotonic_threaded<G, ID, T, R>(make_generator: impl Fn() -> G)
where
    G: UlidGenerator<ID, T, R> + Send + Sync,
    ID: UlidId + Send,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    const THREADS: usize = 8;
    const TOTAL_IDS: usize = 4096 * 256;
    const IDS_PER_THREAD: usize = TOTAL_IDS / THREADS;

    let generator = Arc::new(make_generator());
    let seen_ids = Arc::new(Mutex::new(HashSet::with_capacity(TOTAL_IDS)));

    scope(|s| {
        for _ in 0..THREADS {
            let generator = Arc::clone(&generator);
            let seen_ids = Arc::clone(&seen_ids);

            s.spawn(move || {
                for _ in 0..IDS_PER_THREAD {
                    loop {
                        match generator.try_next_id().unwrap() {
                            IdGenStatus::Ready { id } => {
                                assert!(seen_ids.lock().unwrap().insert(id));
                                break;
                            }
                            IdGenStatus::Pending { .. } => std::thread::yield_now(),
                        }
                    }
                }
            });
        }
    });

    let final_count = seen_ids.lock().unwrap().len();
    assert_eq!(final_count, TOTAL_IDS, "Expected {TOTAL_IDS} unique IDs");
}

#[test]
fn basic_generator() {
    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 43 };
    let generator: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(mock_time, mock_rand);
    let id = generator.next_id().unwrap_ready();
    assert_eq!(id.timestamp(), 42);
    assert_eq!(id.random(), 43);
}

#[test]
fn basic_generator_mono_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 42 };
    let generator: BasicMonoUlidGenerator<ULID, _, _> =
        BasicMonoUlidGenerator::new(mock_time, mock_rand);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[test]
fn lock_generator_mono_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 42 };

    let generator: LockMonoUlidGenerator<ULID, _, _> =
        LockMonoUlidGenerator::new(mock_time, mock_rand);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[test]
#[cfg(target_has_atomic = "128")]
fn atomic_generator_mono_sequence_test() {
    use crate::generator::AtomicMonoUlidGenerator;

    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 42 };

    let generator: AtomicMonoUlidGenerator<ULID, _, _> =
        AtomicMonoUlidGenerator::new(mock_time, mock_rand);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[test]
fn basic_generator_mono_pending_test() {
    let generator: BasicMonoUlidGenerator<ULID, _, _> =
        BasicMonoUlidGenerator::from_components(0, ULID::max_random(), FixedTime, MinRand);
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[test]
fn lock_generator_mono_pending_test() {
    let generator: LockMonoUlidGenerator<ULID, _, _> =
        LockMonoUlidGenerator::from_components(0, ULID::max_random(), FixedTime, MinRand);
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[test]
#[cfg(target_has_atomic = "128")]
fn atomic_generator_mono_pending_test() {
    use crate::generator::AtomicMonoUlidGenerator;

    let generator: AtomicMonoUlidGenerator<ULID, _, _> =
        AtomicMonoUlidGenerator::from_components(0, ULID::max_random(), FixedTime, MinRand);
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[test]
fn basic_generator_mono_rollover_test() {
    let shared_time = SharedMockStepTime::new(vec![42, 43], 0);
    let generator: BasicMonoUlidGenerator<ULID, _, _> =
        BasicMonoUlidGenerator::new(shared_time.clone(), MaxRand);
    run_generator_handles_rollover(&generator, &shared_time);
}

#[test]
fn lock_generator_mono_rollover_test() {
    let shared_time = SharedMockStepTime::new(vec![42, 43], 0);
    let generator: LockMonoUlidGenerator<ULID, _, _> =
        LockMonoUlidGenerator::new(shared_time.clone(), MaxRand);
    run_generator_handles_rollover(&generator, &shared_time);
}

#[test]
#[cfg(target_has_atomic = "128")]
fn atomic_generator_mono_rollover_test() {
    use crate::generator::AtomicMonoUlidGenerator;

    let shared_time = SharedMockStepTime::new(vec![42, 43], 0);
    let generator: AtomicMonoUlidGenerator<ULID, _, _> =
        AtomicMonoUlidGenerator::new(shared_time.clone(), MaxRand);
    run_generator_handles_rollover(&generator, &shared_time);
}

#[test]
fn basic_generator_monotonic_clock_random_increments() {
    let clock = MonotonicClock::default();
    let rand = ThreadRandom;
    let generator: BasicMonoUlidGenerator<ULID, _, _> = BasicMonoUlidGenerator::new(clock, rand);
    run_generator_monotonic(&generator);
}

#[test]
fn lock_generator_monotonic_clock_random_increments() {
    let clock = MonotonicClock::default();
    let rand = ThreadRandom;
    let generator: LockMonoUlidGenerator<ULID, _, _> = LockMonoUlidGenerator::new(clock, rand);
    run_generator_monotonic(&generator);
}

#[test]
#[cfg(target_has_atomic = "128")]
fn atomic_generator_monotonic_clock_random_increments() {
    use crate::generator::AtomicMonoUlidGenerator;

    let clock = MonotonicClock::default();
    let rand = ThreadRandom;
    let generator: AtomicMonoUlidGenerator<ULID, _, _> = AtomicMonoUlidGenerator::new(clock, rand);
    run_generator_monotonic(&generator);
}

#[test]
fn lock_generator_threaded_monotonic() {
    let clock = MonotonicClock::default();
    let rand = ThreadRandom;
    run_generator_monotonic_threaded(move || {
        LockMonoUlidGenerator::<ULID, _, _>::new(clock.clone(), rand.clone())
    });
}

#[test]
#[cfg(target_has_atomic = "128")]
fn atomic_generator_threaded_monotonic() {
    use crate::generator::AtomicMonoUlidGenerator;

    let clock = MonotonicClock::default();
    let rand = ThreadRandom;
    run_generator_monotonic_threaded(move || {
        AtomicMonoUlidGenerator::<ULID, _, _>::new(clock.clone(), rand.clone())
    });
}

#[cfg(not(feature = "parking-lot"))]
#[test]
fn lock_is_poisoned_on_panic_std_mutex() {
    let clock = MonotonicClock::default();
    let rand = ThreadRandom;

    let generator: LockMonoUlidGenerator<ULID, _, _> = LockMonoUlidGenerator::new(clock, rand);

    {
        let state = Arc::clone(&generator.state);
        let _ = thread::spawn(move || {
            let _g = state.lock();
            panic!("boom: poison the mutex");
        })
        .join();
    }

    let err = generator
        .try_next_id()
        .expect_err("expected an error after poison");
    assert!(matches!(err, crate::generator::Error::LockPoisoned));
}

#[cfg(feature = "parking-lot")]
#[test]
#[should_panic(expected = "parking_lot::Mutex cannot be poisoned")]
fn lock_is_poisoned_on_panic_parking_lot_mutex() {
    let clock = MonotonicClock::default();
    let rand = ThreadRandom;

    let generator: LockMonoUlidGenerator<ULID, _, _> = LockMonoUlidGenerator::new(clock, rand);

    {
        let state = Arc::clone(&generator.state);
        let _ = thread::spawn(move || {
            let _g = state.lock();
            panic!("boom: poison the mutex");
        })
        .join();
    }

    generator
        .try_next_id()
        .expect_err("parking_lot::Mutex cannot be poisoned");
}

#[cfg(feature = "parking-lot")]
#[test]
fn lock_can_call_next_id() {
    let clock = MonotonicClock::default();
    let rand = ThreadRandom;

    let generator: LockMonoUlidGenerator<ULID, _, _> = LockMonoUlidGenerator::new(clock, rand);

    {
        let status = thread::spawn(move || generator.next_id()).join().unwrap();
        assert!(matches!(status, IdGenStatus::Ready { .. }));
    }
}
