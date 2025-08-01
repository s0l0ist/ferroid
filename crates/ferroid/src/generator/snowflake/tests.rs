use crate::{
    AtomicSnowflakeGenerator, BasicSnowflakeGenerator, Id, IdGenStatus, LockSnowflakeGenerator,
    MonotonicClock, Snowflake, SnowflakeGenerator, SnowflakeTwitterId, TimeSource, ToU64,
};
use alloc::rc::Rc;
use alloc::sync::Arc;
use alloc::{vec, vec::Vec};
use core::cell::Cell;
use std::collections::HashSet;
use std::sync::Mutex;
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
        u128::from(<Self as TimeSource<u64>>::current_millis(self))
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
            Self::Ready { id } => panic!("unexpected ready ({id})"),
            Self::Pending { yield_for } => yield_for,
        }
    }
}

fn run_id_sequence_increments_within_same_tick<G, ID, T>(generator: &G)
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    let id1 = generator.next_id().unwrap_ready();
    let id2 = generator.next_id().unwrap_ready();
    let id3 = generator.next_id().unwrap_ready();

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
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    let yield_for = generator.next_id().unwrap_pending();
    assert_eq!(yield_for, ID::ONE);
}

fn run_generator_handles_rollover<G, ID, T>(generator: &G, shared_time: &SharedMockStepTime)
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    for i in 0..=ID::max_sequence().to_u64() {
        let id = generator.next_id().unwrap_ready();
        assert_eq!(id.sequence().to_u64(), i);
        assert_eq!(id.timestamp().to_u64(), 42);
    }

    let yield_for = generator.next_id().unwrap_pending();
    assert_eq!(yield_for, ID::ONE);

    shared_time.clock.index.set(1);

    let id = generator.next_id().unwrap_ready();
    assert_eq!(id.timestamp().to_u64(), 43);
    assert_eq!(id.sequence().to_u64(), 0);
}

fn run_generator_monotonic<G, ID, T>(generator: &G)
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    let mut last_timestamp = ID::ZERO;
    let mut sequence = ID::ZERO;
    #[allow(clippy::items_after_statements)]
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
                    assert_eq!(id.machine_id(), ID::ONE);
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

fn run_generator_monotonic_threaded<G, ID, T>(make_generator: impl Fn() -> G)
where
    G: SnowflakeGenerator<ID, T> + Send + Sync,
    ID: Snowflake + Send,
    T: TimeSource<ID::Ty>,
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
fn basic_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[test]
fn lock_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(&generator);
}

#[test]
fn atomic_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(&generator);
}

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

#[test]
fn atomic_generator_pending_test() {
    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::from_components(
            0,
            0,
            SnowflakeTwitterId::max_sequence(),
            FixedTime,
        );
    run_generator_returns_pending_when_sequence_exhausted(&generator);
}

#[test]
fn basic_generator_rollover_test() {
    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 43],
            index: Cell::new(0),
        }),
    };
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(&generator, &shared_time);
}

#[test]
fn lock_generator_rollover_test() {
    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 43],
            index: Cell::new(0),
        }),
    };
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(&generator, &shared_time);
}

#[test]
fn atomic_generator_rollover_test() {
    let shared_time = SharedMockStepTime {
        clock: Rc::new(MockStepTime {
            values: vec![42, 43],
            index: Cell::new(0),
        }),
    };
    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(&generator, &shared_time);
}

#[test]
fn basic_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::new(1, clock);
    run_generator_monotonic(&generator);
}

#[test]
fn lock_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(1, clock);
    run_generator_monotonic(&generator);
}

#[test]
fn atomic_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::new(1, clock);
    run_generator_monotonic(&generator);
}

#[test]
fn lock_generator_threaded_monotonic() {
    let clock = MonotonicClock::default();
    run_generator_monotonic_threaded(move || {
        LockSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, clock.clone())
    });
}

#[test]
fn atomic_generator_threaded_monotonic() {
    let clock = MonotonicClock::default();
    run_generator_monotonic_threaded(move || {
        AtomicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, clock.clone())
    });
}
