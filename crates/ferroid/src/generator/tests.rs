use crate::{
    BasicUlidGenerator, IdGenStatus, MonotonicClock, RandSource, Snowflake, SnowflakeTwitterId,
    TimeSource, ToU64, ULID, Ulid, UlidGenerator,
    generator::{
        AtomicSnowflakeGenerator, BasicSnowflakeGenerator, LockSnowflakeGenerator,
        SnowflakeGenerator,
    },
    random_native::ThreadRandom,
};
use core::{cell::Cell, fmt, hash::Hash};
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

// Mock RNG for deterministic testing
struct MockRng {
    value: u128,
}

impl RandSource<u128> for MockRng {
    fn rand(&self) -> u128 {
        self.value
    }
}

// Counter RNG that increments each time
struct CounterRng {
    counter: Cell<u128>,
}

impl CounterRng {
    fn new() -> Self {
        Self {
            counter: Cell::new(0),
        }
    }
}

impl RandSource<u128> for CounterRng {
    fn rand(&self) -> u128 {
        let curr = self.counter.get();
        self.counter.set(curr + 1);
        curr
    }
}

fn run_id_sequence_increments_within_same_tick<G, ID, T>(generator: G)
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake + fmt::Debug + fmt::Display,
    T: TimeSource<ID::Ty>,
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

fn run_generator_returns_pending_when_sequence_exhausted<G, ID, T>(generator: G)
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake + fmt::Debug + fmt::Display,
    T: TimeSource<ID::Ty>,
{
    let yield_for = generator.next_id().unwrap_pending();
    assert_eq!(yield_for, ID::ONE);
}

fn run_generator_handles_rollover<G, ID, T>(generator: G, shared_time: SharedMockStepTime)
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake + fmt::Debug + fmt::Display,
    T: TimeSource<ID::Ty>,
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

fn run_generator_monotonic<G, ID, T>(generator: G)
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake + fmt::Debug,
    T: TimeSource<ID::Ty>,
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
    ID: Snowflake + PartialEq + Eq + Hash + Send,
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

#[test]
fn basic_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(generator);
}

#[test]
fn lock_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(generator);
}

#[test]
fn atomic_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(generator);
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
    run_generator_returns_pending_when_sequence_exhausted(generator);
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
    run_generator_returns_pending_when_sequence_exhausted(generator);
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
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::new(1, shared_time.clone());
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
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(generator, shared_time);
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
    run_generator_handles_rollover(generator, shared_time);
}

#[test]
fn basic_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator: BasicSnowflakeGenerator<SnowflakeTwitterId, _> =
        BasicSnowflakeGenerator::new(1, clock);
    run_generator_monotonic(generator);
}

#[test]
fn lock_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
        LockSnowflakeGenerator::new(1, clock);
    run_generator_monotonic(generator);
}

#[test]
fn atomic_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator: AtomicSnowflakeGenerator<SnowflakeTwitterId, _> =
        AtomicSnowflakeGenerator::new(1, clock);
    run_generator_monotonic(generator);
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

trait IdGenStatusExt<T>
where
    T: Snowflake + fmt::Display,
    T::Ty: fmt::Display,
{
    fn unwrap_ready(self) -> T;
    fn unwrap_pending(self) -> T::Ty;
}

impl<T> IdGenStatusExt<T> for IdGenStatus<T>
where
    T: Snowflake + fmt::Display,
    T::Ty: fmt::Display,
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

fn run_ulid_ids_have_correct_timestamp<G, ID, T, R>(generator: G, expected_timestamp: ID::Ty)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid + fmt::Debug,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let id1 = generator.next_id();
    let id2 = generator.next_id();
    let id3 = generator.next_id();

    assert_eq!(id1.timestamp(), expected_timestamp);
    assert_eq!(id2.timestamp(), expected_timestamp);
    assert_eq!(id3.timestamp(), expected_timestamp);
}

fn run_ulid_ids_have_different_random_components<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid + fmt::Debug + PartialEq,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let id1 = generator.next_id();
    let id2 = generator.next_id();
    let id3 = generator.next_id();

    // With a counter RNG, random components should be different
    assert_ne!(id1.randomness(), id2.randomness());
    assert_ne!(id2.randomness(), id3.randomness());
    assert_ne!(id1.randomness(), id3.randomness());

    // But timestamps should be the same (using fixed time)
    assert_eq!(id1.timestamp(), id2.timestamp());
    assert_eq!(id2.timestamp(), id3.timestamp());
}

fn run_ulid_try_next_id_never_fails<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid + fmt::Debug,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    // Generate many IDs - should never fail
    for _ in 0..1000 {
        let result = generator.try_next_id();
        assert!(
            result.is_ok(),
            "try_next_id should never fail for Ulid generators"
        );
    }
}

fn run_ulid_ids_are_unique_with_real_rng<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid + fmt::Debug + Clone + std::hash::Hash + Eq,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut seen = HashSet::new();
    const NUM_IDS: usize = 10000;

    for _ in 0..NUM_IDS {
        let id = generator.next_id();
        assert!(seen.insert(id.clone()), "Generated duplicate ID: {:?}", id);
    }

    assert_eq!(seen.len(), NUM_IDS);
}

fn run_ulid_ids_are_time_ordered<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid + fmt::Debug,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut last_timestamp = None;

    for _ in 0..100 {
        let id = generator.next_id();
        let timestamp = id.timestamp();

        if let Some(last_ts) = last_timestamp {
            // Timestamps should be monotonically non-decreasing
            assert!(
                timestamp >= last_ts,
                "Timestamp went backwards: {} < {}",
                timestamp,
                last_ts
            );
        }

        last_timestamp = Some(timestamp);

        // Small delay to ensure time advances
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn basic_ulid_generator_timestamp_test() {
    let mock_time = MockTime { millis: 1234567890 };
    let mock_rng = MockRng { value: 42 };
    let generator: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(mock_time, mock_rng);

    run_ulid_ids_have_correct_timestamp(generator, 1234567890);
}

#[test]
fn basic_ulid_generator_random_component_test() {
    let mock_time = MockTime { millis: 42 };
    let counter_rng = CounterRng::new();
    let generator: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(mock_time, counter_rng);

    run_ulid_ids_have_different_random_components(generator);
}

#[test]
fn basic_ulid_generator_try_next_id_test() {
    let mock_time = MockTime { millis: 42 };
    let mock_rng = MockRng { value: 123 };
    let generator: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(mock_time, mock_rng);

    run_ulid_try_next_id_never_fails(generator);
}

#[test]
fn basic_ulid_generator_uniqueness_test() {
    let clock = MonotonicClock::default();
    let rng = ThreadRandom::default();
    let generator: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(clock, rng);

    run_ulid_ids_are_unique_with_real_rng(generator);
}

#[test]
fn basic_ulid_generator_time_ordering_test() {
    let clock = MonotonicClock::default();
    let rng = ThreadRandom::default();
    let generator: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(clock, rng);

    run_ulid_ids_are_time_ordered(generator);
}

#[test]
fn basic_ulid_generator_components_test() {
    let mock_time = MockTime {
        millis: 0x123456789ABC,
    };
    let mock_rng = MockRng {
        value: 0xDEF012345678,
    };
    let generator: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(mock_time, mock_rng);

    let id = generator.next_id();

    // Verify the ID was constructed correctly from components
    assert_eq!(id.timestamp(), 0x123456789ABC);
    assert_eq!(id.randomness(), 0xDEF012345678);

    // Verify we can reconstruct the ID from its components
    let reconstructed = ULID::from_components(id.timestamp(), id.randomness());
    assert_eq!(id, reconstructed);
}

#[test]
fn basic_ulid_generator_deterministic_test() {
    // Two generators with identical time and RNG should produce identical sequences
    let mock_time1 = MockTime { millis: 42 };
    let mock_rng1 = CounterRng::new();
    let generator1: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(mock_time1, mock_rng1);

    let mock_time2 = MockTime { millis: 42 };
    let mock_rng2 = CounterRng::new();
    let generator2: BasicUlidGenerator<ULID, _, _> = BasicUlidGenerator::new(mock_time2, mock_rng2);

    for _ in 0..10 {
        let id1 = generator1.next_id();
        let id2 = generator2.next_id();
        assert_eq!(
            id1, id2,
            "Generators with identical inputs should produce identical outputs"
        );
    }
}
