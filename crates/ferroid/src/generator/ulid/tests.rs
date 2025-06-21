use crate::{
    BasicUlidGenerator, Id, IdGenStatus, LockUlidGenerator, MonotonicClock, RandSource,
    ThreadRandom, TimeSource, ToU64, ULID_MONO, Ulid, UlidGenerator,
};
use core::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread::scope;

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
        <Self as TimeSource<u64>>::current_millis(self) as u128
    }
}

struct MockRand {
    rand: u64,
}

impl RandSource<u64> for MockRand {
    fn rand(&self) -> u64 {
        self.rand
    }
}

impl RandSource<u128> for MockRand {
    fn rand(&self) -> u128 {
        <Self as RandSource<u64>>::rand(self) as u128
    }
}

#[derive(Clone)]
struct SharedMockStepTime {
    clock: Rc<MockStepTime>,
}

impl TimeSource<u64> for SharedMockStepTime {
    fn current_millis(&self) -> u64 {
        self.clock.values[self.clock.index.get()]
    }
}

impl TimeSource<u128> for SharedMockStepTime {
    fn current_millis(&self) -> u128 {
        <Self as TimeSource<u64>>::current_millis(self) as u128
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

impl TimeSource<u128> for FixedTime {
    fn current_millis(&self) -> u128 {
        0
    }
}

struct FixedRand;
impl RandSource<u64> for FixedRand {
    fn rand(&self) -> u64 {
        0
    }
}

impl RandSource<u128> for FixedRand {
    fn rand(&self) -> u128 {
        0
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
            IdGenStatus::Ready { id } => id,
            IdGenStatus::Pending { yield_for } => {
                panic!("unexpected pending (yield for: {})", yield_for)
            }
        }
    }

    fn unwrap_pending(self) -> T::Ty {
        match self {
            IdGenStatus::Ready { id } => panic!("unexpected ready ({})", id),
            IdGenStatus::Pending { yield_for } => yield_for,
        }
    }
}

fn run_id_sequence_increments_within_same_tick<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let id1 = generator.next_id().unwrap_ready();
    let id2 = generator.next_id().unwrap_ready();
    let id3 = generator.next_id().unwrap_ready();

    assert_eq!(id1.timestamp().to_u64().unwrap(), 42);
    assert_eq!(id2.timestamp().to_u64().unwrap(), 42);
    assert_eq!(id3.timestamp().to_u64().unwrap(), 42);
    assert_eq!(id1.sequence().to_u64().unwrap(), 0);
    assert_eq!(id2.sequence().to_u64().unwrap(), 1);
    assert_eq!(id3.sequence().to_u64().unwrap(), 2);
    assert!(id1 < id2 && id2 < id3);
}

fn run_generator_returns_pending_when_sequence_exhausted<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let yield_for = generator.next_id().unwrap_pending();
    assert_eq!(yield_for, ID::ONE);
}

fn run_generator_handles_rollover<G, ID, T, R>(generator: G, shared_time: SharedMockStepTime)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    for i in 0..=ID::max_sequence().to_u64().unwrap() {
        let id = generator.next_id().unwrap_ready();
        assert_eq!(id.sequence().to_u64().unwrap(), i);
        assert_eq!(id.timestamp().to_u64().unwrap(), 42);
    }

    let yield_for = generator.next_id().unwrap_pending();
    assert_eq!(yield_for, ID::ONE);

    shared_time.clock.index.set(1);

    let id = generator.next_id().unwrap_ready();
    assert_eq!(id.timestamp().to_u64().unwrap(), 43);
    assert_eq!(id.sequence().to_u64().unwrap(), 0);
}

fn run_generator_monotonic<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut last_timestamp = ID::ZERO;
    let mut sequence = ID::ZERO;
    const TOTAL_IDS: usize = 4096 * 256;

    for _ in 0..TOTAL_IDS {
        loop {
            match generator.next_id() {
                IdGenStatus::Ready { id } => {
                    let ts = id.timestamp();
                    if ts > last_timestamp {
                        sequence = ID::ZERO;
                    }

                    assert!(ts >= last_timestamp);
                    assert_eq!(id.random(), ID::ZERO);
                    assert_eq!(id.sequence(), sequence);

                    last_timestamp = ts;
                    sequence += ID::ONE;
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
    ID: Ulid + Send,
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
                        match generator.next_id() {
                            IdGenStatus::Ready { id } => {
                                let mut set = seen_ids.lock().unwrap();
                                assert!(set.insert(id));
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
    assert_eq!(final_count, TOTAL_IDS, "Expected {} unique IDs", TOTAL_IDS);
}

#[test]
fn basic_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 42 };
    let generator: BasicUlidGenerator<ULID_MONO, _, _> =
        BasicUlidGenerator::new(mock_time, mock_rand);
    run_id_sequence_increments_within_same_tick(generator);
}

#[test]
fn lock_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let mock_rand = MockRand { rand: 42 };

    let generator: LockUlidGenerator<ULID_MONO, _, _> =
        LockUlidGenerator::new(mock_time, mock_rand);
    run_id_sequence_increments_within_same_tick(generator);
}

#[test]
fn basic_generator_pending_test() {
    let generator: BasicUlidGenerator<ULID_MONO, _, _> =
        BasicUlidGenerator::from_components(0, 0, ULID_MONO::max_sequence(), FixedTime, FixedRand);
    run_generator_returns_pending_when_sequence_exhausted(generator);
}

#[test]
fn lock_generator_pending_test() {
    let generator: LockUlidGenerator<ULID_MONO, _, _> =
        LockUlidGenerator::from_components(0, 0, ULID_MONO::max_sequence(), FixedTime, FixedRand);
    run_generator_returns_pending_when_sequence_exhausted(generator);
}

#[test]
fn basic_generator_rollover_test() {
    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 43],
            index: Cell::new(0),
        }),
    };
    let generator: BasicUlidGenerator<ULID_MONO, _, _> =
        BasicUlidGenerator::new(shared_time.clone(), FixedRand);
    run_generator_handles_rollover(generator, shared_time);
}

#[test]
fn lock_generator_rollover_test() {
    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 43],
            index: Cell::new(0),
        }),
    };
    let generator: LockUlidGenerator<ULID_MONO, _, _> =
        LockUlidGenerator::new(shared_time.clone(), FixedRand);
    run_generator_handles_rollover(generator, shared_time);
}

#[test]
fn basic_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator: BasicUlidGenerator<ULID_MONO, _, _> = BasicUlidGenerator::new(clock, FixedRand);
    run_generator_monotonic(generator);
}

#[test]
fn lock_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator: LockUlidGenerator<ULID_MONO, _, _> = LockUlidGenerator::new(clock, FixedRand);
    run_generator_monotonic(generator);
}

#[test]
fn lock_generator_threaded_monotonic() {
    let clock = MonotonicClock::default();
    let rand = ThreadRandom::default();
    run_generator_monotonic_threaded(move || {
        LockUlidGenerator::<ULID_MONO, _, _>::new(clock.clone(), rand.clone())
    });
}
