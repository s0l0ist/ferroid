use crate::{
    BasicUlidGenerator, Id, IdGenStatus, MonotonicClock, RandSource, ThreadRandom, TimeSource,
    ULID, Ulid, UlidGenerator,
};
use core::cell::Cell;
use std::collections::HashSet;

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

trait IdGenStatusExt<T>
where
    T: Id,
{
    fn unwrap_ready(self) -> T;
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

fn run_ulid_ids_have_correct_timestamp<G, ID, T, R>(generator: G, expected_timestamp: ID::Ty)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let id1 = generator.next_id().unwrap_ready();
    let id2 = generator.next_id().unwrap_ready();
    let id3 = generator.next_id().unwrap_ready();

    assert_eq!(id1.timestamp(), expected_timestamp);
    assert_eq!(id2.timestamp(), expected_timestamp);
    assert_eq!(id3.timestamp(), expected_timestamp);
}

fn run_ulid_ids_have_different_random_components<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let id1 = generator.next_id().unwrap_ready();
    let id2 = generator.next_id().unwrap_ready();
    let id3 = generator.next_id().unwrap_ready();

    // With a counter RNG, random components should be different
    assert_ne!(id1.random(), id2.random());
    assert_ne!(id2.random(), id3.random());
    assert_ne!(id1.random(), id3.random());

    // But timestamps should be the same (using fixed time)
    assert_eq!(id1.timestamp(), id2.timestamp());
    assert_eq!(id2.timestamp(), id3.timestamp());
}

fn run_ulid_try_next_id_never_fails<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid,
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
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut seen = HashSet::new();
    const NUM_IDS: usize = 10000;

    for _ in 0..NUM_IDS {
        let id = generator.next_id().unwrap_ready();
        assert!(seen.insert(id.clone()), "Generated duplicate ID: {:?}", id);
    }

    assert_eq!(seen.len(), NUM_IDS);
}

fn run_ulid_ids_are_time_ordered<G, ID, T, R>(generator: G)
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut last_timestamp = None;

    for _ in 0..100 {
        let id = generator.next_id().unwrap_ready();
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

    let id = generator.next_id().unwrap_ready();

    // Verify the ID was constructed correctly from components
    assert_eq!(id.timestamp(), 0x123456789ABC);
    assert_eq!(id.random(), 0xDEF012345678);

    // Verify we can reconstruct the ID from its components
    let reconstructed = ULID::from_components(id.timestamp(), id.random());
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
