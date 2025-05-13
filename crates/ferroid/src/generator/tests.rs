use crate::{
    generator::{
        AtomicSnowflakeGenerator, BasicSnowflakeGenerator, LockSnowflakeGenerator,
        MultithreadedSnowflakeGenerator, SnowflakeGenerator,
    },
    IdGenStatus, MonotonicClock, Snowflake, SnowflakeDiscordId, SnowflakeTwitterId, TimeSource,
};
use core::fmt;
use std::cell::Cell;
use std::rc::Rc;

struct MockTime {
    millis: u64,
}

impl TimeSource<u64> for MockTime {
    fn current_millis(&self) -> u64 {
        self.millis
    }
}

fn run_id_sequence_increments_within_same_tick<G>(mut generator: G)
where
    G: SnowflakeGenerator<SnowflakeTwitterId>,
{
    let id1 = generator.next().unwrap_ready();
    let id2 = generator.next().unwrap_ready();
    let id3 = generator.next().unwrap_ready();

    assert_eq!(id1.timestamp(), 42);
    assert_eq!(id2.timestamp(), 42);
    assert_eq!(id3.timestamp(), 42);
    assert_eq!(id1.sequence(), 0);
    assert_eq!(id2.sequence(), 1);
    assert_eq!(id3.sequence(), 2);
    assert!(id1 < id2 && id2 < id3);
}

fn run_generator_returns_pending_when_sequence_exhausted<G>(mut generator: G)
where
    G: SnowflakeGenerator<SnowflakeTwitterId>,
{
    let yield_until = generator.next().unwrap_pending();
    assert_eq!(yield_until, 1);
}

fn run_generator_handles_rollover<G>(mut generator: G, shared_time: Rc<MockStepTime>)
where
    G: SnowflakeGenerator<SnowflakeTwitterId>,
{
    for i in 0..=SnowflakeTwitterId::max_sequence() {
        let id = generator.next().unwrap_ready();
        assert_eq!(id.sequence(), i);
        assert_eq!(id.timestamp(), 42);
    }

    let yield_until = generator.next().unwrap_pending();
    assert_eq!(yield_until, 43);

    shared_time.index.set(1);

    let id = generator.next().unwrap_ready();
    assert_eq!(id.timestamp(), 43);
    assert_eq!(id.sequence(), 0);
}

fn run_generator_monotonic<G>(mut generator: G)
where
    G: SnowflakeGenerator<SnowflakeTwitterId>,
{
    let mut last_timestamp = 0;
    let mut sequence = 0;

    for _ in 0..8192 {
        loop {
            match generator.next() {
                IdGenStatus::Ready { id } => {
                    let ts = id.timestamp();
                    if ts > last_timestamp {
                        sequence = 0;
                    }

                    assert!(ts >= last_timestamp);
                    assert_eq!(id.machine_id(), 1);
                    assert_eq!(id.sequence(), sequence);

                    last_timestamp = ts;
                    sequence += 1;
                    break;
                }
                IdGenStatus::Pending { .. } => {
                    std::hint::spin_loop();
                }
            }
        }
    }
}

fn run_generator_monotonic_threaded<G>(make_generator: impl Fn() -> G)
where
    G: MultithreadedSnowflakeGenerator<SnowflakeTwitterId> + Sync + Send,
{
    use std::collections::HashSet;
    use std::sync::{Arc, Mutex};
    use std::thread::scope;

    const THREADS: usize = 8;
    const TOTAL_IDS: usize = 4096;
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
                        match generator.next() {
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

struct MockStepTime {
    values: Vec<u64>,
    index: Cell<usize>,
}

impl TimeSource<u64> for Rc<MockStepTime> {
    fn current_millis(&self) -> u64 {
        self.values[self.index.get()]
    }
}

#[test]
fn basic_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator = BasicSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(generator);
}

#[test]
fn lock_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator = LockSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(generator);
}

#[test]
fn atomic_generator_sequence_test() {
    let mock_time = MockTime { millis: 42 };
    let generator = AtomicSnowflakeGenerator::new(0, mock_time);
    run_id_sequence_increments_within_same_tick(generator);
}

#[test]
fn basic_generator_pending_test() {
    struct FixedTime;
    impl TimeSource<u64> for FixedTime {
        fn current_millis(&self) -> u64 {
            0
        }
    }
    let generator = BasicSnowflakeGenerator::from_components(
        0,
        0,
        SnowflakeTwitterId::max_sequence(),
        FixedTime,
    );
    run_generator_returns_pending_when_sequence_exhausted(generator);
}

#[test]
fn lock_generator_pending_test() {
    struct FixedTime;
    impl TimeSource<u64> for FixedTime {
        fn current_millis(&self) -> u64 {
            0
        }
    }
    let generator = LockSnowflakeGenerator::from_components(
        0,
        0,
        SnowflakeTwitterId::max_sequence(),
        FixedTime,
    );
    run_generator_returns_pending_when_sequence_exhausted(generator);
}

#[test]
fn atomic_generator_pending_test() {
    struct FixedTime;
    impl TimeSource<u64> for FixedTime {
        fn current_millis(&self) -> u64 {
            0
        }
    }
    let generator = AtomicSnowflakeGenerator::from_components(
        0,
        0,
        SnowflakeTwitterId::max_sequence(),
        FixedTime,
    );
    run_generator_returns_pending_when_sequence_exhausted(generator);
}

#[test]
fn basic_generator_rollover_test() {
    let shared_time = Rc::new(MockStepTime {
        values: vec![42, 43],
        index: Cell::new(0),
    });
    let generator = BasicSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(generator, shared_time);
}

#[test]
fn lock_generator_rollover_test() {
    let shared_time = Rc::new(MockStepTime {
        values: vec![42, 43],
        index: Cell::new(0),
    });
    let generator = LockSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(generator, shared_time);
}

#[test]
fn atomic_generator_rollover_test() {
    let shared_time = Rc::new(MockStepTime {
        values: vec![42, 43],
        index: Cell::new(0),
    });
    let generator = AtomicSnowflakeGenerator::new(1, shared_time.clone());
    run_generator_handles_rollover(generator, shared_time);
}

#[test]
fn basic_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator = BasicSnowflakeGenerator::new(1, clock);
    run_generator_monotonic(generator);
}

#[test]
fn lock_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator = LockSnowflakeGenerator::new(1, clock);
    run_generator_monotonic(generator);
}

#[test]
fn atomic_generator_monotonic_clock_sequence_increments() {
    let clock = MonotonicClock::default();
    let generator = AtomicSnowflakeGenerator::new(1, clock);
    run_generator_monotonic(generator);
}

#[test]
fn lock_generator_threaded_monotonic() {
    let clock = MonotonicClock::default();
    run_generator_monotonic_threaded(move || LockSnowflakeGenerator::new(0, clock));
}

#[test]
fn atomic_generator_threaded_monotonic() {
    let clock = MonotonicClock::default();
    run_generator_monotonic_threaded(move || AtomicSnowflakeGenerator::new(0, clock));
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
            IdGenStatus::Pending { yield_until } => {
                panic!("unexpected pending (yield until: {})", yield_until)
            }
        }
    }

    fn unwrap_pending(self) -> T::Ty {
        match self {
            IdGenStatus::Ready { id } => panic!("unexpected ready ({})", id),
            IdGenStatus::Pending { yield_until } => yield_until,
        }
    }
}

#[test]
fn snowflake_discord_id_extraction() {
    let id = SnowflakeDiscordId::from(123456, 18, 4095);
    assert_eq!(id.timestamp(), 123456);
    assert_eq!(id.machine_id(), 18);
    assert_eq!(id.sequence(), 4095);
    assert_eq!(SnowflakeDiscordId::from_components(123456, 18, 4095), id);
}
