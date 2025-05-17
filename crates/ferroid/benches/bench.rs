use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use ferroid::{
    AtomicSnowflakeGenerator, BasicSnowflakeGenerator, IdGenStatus, LockSnowflakeGenerator,
    MonotonicClock, MultithreadedSnowflakeGenerator, Snowflake, SnowflakeGenerator,
    SnowflakeTwitterId, TimeSource,
};
use std::sync::{Arc, Barrier};
use std::thread::{scope, yield_now};
use std::time::Instant;

struct FixedMockTime {
    millis: u64,
}

impl TimeSource<u64> for FixedMockTime {
    fn current_millis(&self) -> u64 {
        self.millis
    }
}

// Total number of IDs generated per benchmark iteration. Threads divide this
// equally among themselves in multithreaded scenarios.
const TOTAL_IDS: usize = 4096;

/// Benchmarks a sequential generator where all generated IDs are `Ready` (no
/// contention, no `Pending`). This simulates the generator's hot path under
/// ideal conditions which is useful for optimizing the generator itself.
fn bench_generator<G, ID, T>(c: &mut Criterion, group_name: &str, generator_factory: impl Fn() -> G)
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);

    group.throughput(Throughput::Elements(TOTAL_IDS as u64));
    group.bench_function(format!("elems/{}", TOTAL_IDS), |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();

            for _ in 0..iters {
                let mut generator = generator_factory();
                for _ in 0..TOTAL_IDS {
                    match generator.next() {
                        IdGenStatus::Ready { id } => {
                            black_box(id);
                        }
                        IdGenStatus::Pending { .. } => unreachable!(),
                    }
                }
            }

            start.elapsed()
        });
    });

    group.finish();
}

/// Benchmarks a sequential generator that may yield `Pending` if the timestamp
/// hasn't advanced. This simulates realistic usage with clock-aware generators
/// like `MonotonicClock`.
fn bench_generator_yield<G, ID, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_factory: impl Fn() -> G,
) where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);

    group.throughput(Throughput::Elements(TOTAL_IDS as u64));
    group.bench_function(format!("elems/{}", TOTAL_IDS), |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();

            for _ in 0..iters {
                let mut generator = generator_factory();
                for _ in 0..TOTAL_IDS {
                    loop {
                        match generator.next() {
                            IdGenStatus::Ready { id } => {
                                black_box(id);
                                break;
                            }
                            IdGenStatus::Pending { .. } => std::hint::spin_loop(),
                        }
                    }
                }
            }

            start.elapsed()
        });
    });

    group.finish();
}

/// Benchmarks a multithreaded generator where each thread attempts to generate
/// IDs from a shared generator. This measures contention performance when the
/// generator always returns `Ready` (e.g., fixed timestamp).
fn bench_generator_threaded<G, ID, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn() -> G,
) where
    G: MultithreadedSnowflakeGenerator<ID, T> + Send + Sync,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);
    let thread_counts = [1, 2, 4, 8, 16];

    for thread_count in thread_counts {
        let ids_per_thread = TOTAL_IDS / thread_count;

        group.throughput(Throughput::Elements(TOTAL_IDS as u64));
        group.bench_function(
            format!("threads={}/elems={}", thread_count, TOTAL_IDS),
            |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();

                    for _ in 0..iters {
                        let generator = Arc::new(generator_fn());
                        let barrier = Arc::new(Barrier::new(thread_count + 1));
                        scope(|s| {
                            for _ in 0..thread_count {
                                let generator = Arc::clone(&generator);
                                let barrier = Arc::clone(&barrier);
                                s.spawn(move || {
                                    barrier.wait();
                                    for _ in 0..ids_per_thread {
                                        match generator.next() {
                                            IdGenStatus::Ready { id } => {
                                                black_box(id);
                                            }
                                            IdGenStatus::Pending { .. } => unreachable!(),
                                        }
                                    }
                                });
                            }
                            barrier.wait();
                        });
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks a multithreaded generator that may return `Pending`, simulating
/// contention + time progression. Threads yield when the sequence is exhausted,
/// which is typical with `MonotonicClock`.
fn bench_generator_threaded_yield<G, ID, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn() -> G,
) where
    G: MultithreadedSnowflakeGenerator<ID, T> + Send + Sync,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
    ID: Snowflake,
{
    let mut group = c.benchmark_group(group_name);
    let thread_counts = [1, 2, 4, 8, 16];

    for thread_count in thread_counts {
        let ids_per_thread = TOTAL_IDS / thread_count;

        group.throughput(Throughput::Elements(TOTAL_IDS as u64));
        group.bench_function(
            format!("threads={}/elems={}", thread_count, TOTAL_IDS),
            |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();

                    for _ in 0..iters {
                        let generator = Arc::new(generator_fn());
                        let barrier = Arc::new(Barrier::new(thread_count + 1));
                        scope(|s| {
                            for _ in 0..thread_count {
                                let generator = Arc::clone(&generator);
                                let barrier = Arc::clone(&barrier);
                                s.spawn(move || {
                                    barrier.wait();
                                    for _ in 0..ids_per_thread {
                                        loop {
                                            match generator.next() {
                                                IdGenStatus::Ready { id } => {
                                                    black_box(id);
                                                    break;
                                                }
                                                IdGenStatus::Pending { .. } => yield_now(),
                                            }
                                        }
                                    }
                                });
                            }
                            barrier.wait();
                        });
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks a basic, single-threaded generator using a mock clock that never
/// advances. Always returns `Ready`, so this exercises the fastest possible hot
/// path.
fn benchmark_mock_sequential_basic(c: &mut Criterion) {
    bench_generator::<_, SnowflakeTwitterId, _>(c, "mock/sequential/basic", || {
        BasicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Benchmarks the `LockSnowflakeGenerator` with a mock clock and
/// single-threaded access.
fn benchmark_mock_sequential_lock(c: &mut Criterion) {
    bench_generator::<_, SnowflakeTwitterId, _>(c, "mock/sequential/lock", || {
        LockSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Benchmarks the `AtomicSnowflakeGenerator` with a mock clock and
/// single-threaded access.
fn benchmark_mock_sequential_atomic(c: &mut Criterion) {
    bench_generator::<_, SnowflakeTwitterId, _>(c, "mock/sequential/atomic", || {
        AtomicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Benchmarks the `LockSnowflakeGenerator` under multithreaded contention with
/// a fixed clock. Measures cost of synchronization under no time progression.
fn benchmark_mock_threaded_lock(c: &mut Criterion) {
    bench_generator_threaded::<_, SnowflakeTwitterId, _>(c, "mock/threaded/lock", || {
        LockSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Benchmarks the `AtomicSnowflakeGenerator` under multithreaded contention
/// with a fixed clock. This has to always use yielding because CAS can fail.
fn benchmark_mock_threaded_atomic(c: &mut Criterion) {
    bench_generator_threaded_yield::<_, SnowflakeTwitterId, _>(c, "mock/threaded/atomic", || {
        AtomicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Benchmarks the basic generator with `MonotonicClock` under a single thread.
/// IDs may yield if the clock hasn't advanced; simulates a realistic wall
/// clock.
fn benchmark_mono_sequential_basic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_yield::<_, SnowflakeTwitterId, _>(c, "mono/sequential/basic", || {
        BasicSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Benchmarks `LockSnowflakeGenerator` with `MonotonicClock` under a single
/// thread.
fn benchmark_mono_sequential_lock(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_yield::<_, SnowflakeTwitterId, _>(c, "mono/sequential/lock", || {
        LockSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Benchmarks `AtomicSnowflakeGenerator` with `MonotonicClock` under a single
/// thread.
fn benchmark_mono_sequential_atomic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_yield::<_, SnowflakeTwitterId, _>(c, "mono/sequential/atomic", || {
        AtomicSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Benchmarks the lock-based generator with `MonotonicClock` and multithreaded
/// contention. Threads yield if the sequence is exhausted for the current tick.
fn benchmark_mono_threaded_lock(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_threaded_yield::<_, SnowflakeTwitterId, _>(c, "mono/threaded/lock", || {
        LockSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Benchmarks the atomic generator with `MonotonicClock` and multithreaded
/// contention.
fn benchmark_mono_threaded_atomic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_threaded_yield::<_, SnowflakeTwitterId, _>(c, "mono/threaded/atomic", || {
        AtomicSnowflakeGenerator::new(0, clock.clone())
    });
}

criterion_group!(
    benches,
    // Mock clock
    benchmark_mock_sequential_basic,
    benchmark_mock_sequential_lock,
    benchmark_mock_sequential_atomic,
    benchmark_mock_threaded_lock,
    benchmark_mock_threaded_atomic, // yields because of CAS failures
    // Monotonic clocks (yielding)
    benchmark_mono_sequential_basic,
    benchmark_mono_sequential_lock,
    benchmark_mono_sequential_atomic,
    benchmark_mono_threaded_lock,
    benchmark_mono_threaded_atomic,
);
criterion_main!(benches);
