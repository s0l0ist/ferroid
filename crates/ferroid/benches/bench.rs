use core::hint::black_box;
use criterion::async_executor::SmolExecutor;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ferroid::{
    AtomicSnowflakeGenerator, BasicSnowflakeGenerator, IdGenStatus, LockSnowflakeGenerator,
    MonotonicClock, Result, SmolSleep, Snowflake, SnowflakeGenerator, SnowflakeGeneratorAsyncExt,
    SnowflakeTwitterId, TimeSource, TokioSleep,
};
use futures::future::try_join_all;
use smol::Task;
use std::{
    sync::{Arc, Barrier},
    thread::{scope, yield_now},
    time::Instant,
};
use tokio::runtime::Builder;

struct FixedMockTime {
    millis: u64,
}

impl TimeSource<u64> for FixedMockTime {
    fn current_millis(&self) -> u64 {
        self.millis
    }
}

// Number of IDs generated per benchmark iteration (per-thread for
// multi-threaded).
const TOTAL_IDS: usize = 4096;

/// Benchmarks a hot-path generator where IDs are always `Ready`.
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
                let generator = generator_factory();
                for _ in 0..TOTAL_IDS {
                    match generator.next_id() {
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

/// Benchmarks generators that may yield on clock stall (realistic wallclock
/// behavior).
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
                let generator = generator_factory();
                for _ in 0..TOTAL_IDS {
                    loop {
                        match generator.next_id() {
                            IdGenStatus::Ready { id } => {
                                black_box(id);
                                break;
                            }
                            IdGenStatus::Pending { .. } => core::hint::spin_loop(),
                        }
                    }
                }
            }

            start.elapsed()
        });
    });

    group.finish();
}

/// Benchmarks shared generator across threads, with no yielding (fixed clock).
fn bench_generator_contended<G, ID, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn() -> G,
) where
    G: SnowflakeGenerator<ID, T> + Send + Sync,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);

    for thread_count in [1, 2, 4, 8, 16] {
        let ids_per_thread = TOTAL_IDS / thread_count;

        group.throughput(Throughput::Elements(TOTAL_IDS as u64));
        group.bench_function(
            format!("elems/{}/threads/{}", TOTAL_IDS, thread_count),
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
                                        match generator.next_id() {
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

/// Benchmarks shared generator across threads with yielding on `Pending`.
fn bench_generator_contended_yield<G, ID, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn() -> G,
) where
    G: SnowflakeGenerator<ID, T> + Send + Sync,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);

    for thread_count in [1, 2, 4, 8, 16] {
        let ids_per_thread = TOTAL_IDS / thread_count;

        group.throughput(Throughput::Elements(TOTAL_IDS as u64));
        group.bench_function(
            format!("elems/{}/threads/{}", TOTAL_IDS, thread_count),
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
                                            match generator.next_id() {
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

/// Benchmarks a single async generator on one Tokio thread.
fn bench_generator_sequential_async_tokio<G, ID, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G + Copy,
    clock_factory: impl Fn() -> T + Copy,
) where
    G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
    ID: Snowflake + Send,
    T: TimeSource<ID::Ty> + Clone + Send,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(TOTAL_IDS as u64));

    group.bench_function(format!("elems/{}", TOTAL_IDS), |b| {
        let rt = Builder::new_multi_thread()
            .enable_all()
            .worker_threads(1)
            .build()
            .unwrap();

        b.to_async(&rt).iter_custom(|iters| async move {
            let clock = clock_factory();
            let start = Instant::now();

            for _ in 0..iters {
                let generator = generator_fn(0, clock.clone());
                for _ in 0..TOTAL_IDS {
                    let id = generator.try_next_id_async::<TokioSleep>().await.unwrap();
                    black_box(id);
                }
            }

            start.elapsed()
        });
    });

    group.finish();
}

/// Benchmarks a single async generator on one Smol thread.
fn bench_generator_sequential_async_smol<G, ID, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G + Copy,
    clock_factory: impl Fn() -> T + Copy,
) where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty> + Clone,
{
    unsafe { std::env::remove_var("SMOL_THREADS") };

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(TOTAL_IDS as u64));

    group.bench_function(format!("elems/{}", TOTAL_IDS), |b| {
        b.to_async(SmolExecutor).iter_custom(|iters| async move {
            let clock = clock_factory();
            let start = Instant::now();

            for _ in 0..iters {
                let generator = generator_fn(0, clock.clone());
                for _ in 0..TOTAL_IDS {
                    let id = generator.try_next_id_async::<SmolSleep>().await.unwrap();
                    black_box(id);
                }
            }

            start.elapsed()
        });
    });

    group.finish();
}

/// Benchmarks many async generators in parallel, each running in a separate
/// task.
fn bench_generator_async_tokio<G, ID, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G + Copy,
    clock_factory: impl Fn() -> T + Copy,
) where
    G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
    ID: Snowflake + Send + Sync + 'static,
    T: TimeSource<ID::Ty> + Clone + Send + Sync + 'static,
{
    let mut group = c.benchmark_group(group_name);
    group.sample_size(10);
    group.sampling_mode(criterion::SamplingMode::Flat);

    let total_ids = TOTAL_IDS * 1024;

    for num_generators in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let ids_per_task = total_ids / num_generators;

        group.throughput(Throughput::Elements(total_ids as u64));
        group.bench_function(
            format!("elems/{}/gens/{}", total_ids, num_generators),
            |b| {
                let rt = Builder::new_multi_thread().enable_all().build().unwrap();

                b.to_async(&rt).iter_custom(move |iters| async move {
                    let clock = clock_factory();
                    let start = Instant::now();

                    for _ in 0..iters {
                        let mut tasks: Vec<tokio::task::JoinHandle<Result<()>>> =
                            Vec::with_capacity(num_generators);

                        for i in 0..num_generators {
                            let generator = generator_fn(i as u64, clock.clone());
                            tasks.push(tokio::spawn(async move {
                                for _ in 0..ids_per_task {
                                    let id = generator.try_next_id_async::<TokioSleep>().await?;
                                    black_box(id);
                                }
                                Ok(())
                            }));
                        }

                        for result in try_join_all(tasks).await.unwrap() {
                            result.unwrap();
                        }
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks many async generators in parallel, each running in a separate
/// `smol` task.
pub fn bench_generator_async_smol<G, ID, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G + Copy,
    clock_factory: impl Fn() -> T + Copy,
) where
    G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
    ID: Snowflake + Send,
    T: TimeSource<ID::Ty> + Clone + Send,
{
    // Use all CPUs
    unsafe { std::env::set_var("SMOL_THREADS", num_cpus::get().to_string()) };

    let mut group = c.benchmark_group(group_name);
    group.sample_size(10);
    group.sampling_mode(criterion::SamplingMode::Flat);

    let total_ids = TOTAL_IDS * 1024;

    for num_generators in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let ids_per_task = total_ids / num_generators;

        group.throughput(Throughput::Elements(total_ids as u64));
        group.bench_function(
            format!("elems/{}/gens/{}", total_ids, num_generators),
            |b| {
                b.to_async(SmolExecutor).iter_custom(|iters| async move {
                    let clock = clock_factory();
                    let start = Instant::now();

                    for _ in 0..iters {
                        let mut tasks: Vec<Task<Result<()>>> = Vec::with_capacity(num_generators);

                        for i in 0..num_generators {
                            let generator = generator_fn(i as u64, clock.clone());
                            tasks.push(smol::spawn(async move {
                                for _ in 0..ids_per_task {
                                    let id = generator.try_next_id_async::<SmolSleep>().await?;
                                    black_box(id);
                                }
                                Ok(())
                            }));
                        }

                        try_join_all(tasks).await.unwrap();
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

// --- MOCK CLOCK (fixed, non-advancing time) ---

/// Single-threaded benchmark for `BasicSnowflakeGenerator` with a fixed clock.
/// Always returns `Ready` (no yielding).
fn benchmark_mock_sequential_basic(c: &mut Criterion) {
    bench_generator::<_, SnowflakeTwitterId, _>(c, "mock/sequential/basic", || {
        BasicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Single-threaded benchmark for `LockSnowflakeGenerator` with a fixed clock.
fn benchmark_mock_sequential_lock(c: &mut Criterion) {
    bench_generator::<_, SnowflakeTwitterId, _>(c, "mock/sequential/lock", || {
        LockSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Single-threaded benchmark for `AtomicSnowflakeGenerator` with a fixed clock.
fn benchmark_mock_sequential_atomic(c: &mut Criterion) {
    bench_generator::<_, SnowflakeTwitterId, _>(c, "mock/sequential/atomic", || {
        AtomicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Multithreaded benchmark for `LockSnowflakeGenerator` with a fixed clock. No
/// yielding; measures raw contention.
fn benchmark_mock_contended_lock(c: &mut Criterion) {
    bench_generator_contended::<_, SnowflakeTwitterId, _>(c, "mock/contended/lock", || {
        LockSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Multithreaded benchmark for `AtomicSnowflakeGenerator` with a fixed clock.
/// Threads may yield due to CAS contention.
fn benchmark_mock_contended_atomic(c: &mut Criterion) {
    bench_generator_contended_yield::<_, SnowflakeTwitterId, _>(c, "mock/contended/atomic", || {
        AtomicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

// --- MONOTONIC CLOCK (realistic time with potential yielding) ---

/// Single-threaded benchmark for `BasicSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_sequential_basic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_yield::<_, SnowflakeTwitterId, _>(c, "mono/sequential/basic", || {
        BasicSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Single-threaded benchmark for `LockSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_sequential_lock(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_yield::<_, SnowflakeTwitterId, _>(c, "mono/sequential/lock", || {
        LockSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Single-threaded benchmark for `AtomicSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_sequential_atomic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_yield::<_, SnowflakeTwitterId, _>(c, "mono/sequential/atomic", || {
        AtomicSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Multithreaded benchmark for `LockSnowflakeGenerator` with `MonotonicClock`.
/// Threads yield on sequence exhaustion.
fn benchmark_mono_threaded_lock(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_contended_yield::<_, SnowflakeTwitterId, _>(c, "mono/contended/lock", || {
        LockSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Multithreaded benchmark for `AtomicSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_threaded_atomic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_contended_yield::<_, SnowflakeTwitterId, _>(c, "mono/contended/atomic", || {
        AtomicSnowflakeGenerator::new(0, clock.clone())
    });
}

// --- ASYNC (Tokio) ---

/// Async benchmark for a single `LockSnowflakeGenerator` using `MonotonicClock`
/// for tokio.
fn benchmark_mono_sequential_tokio_lock(c: &mut Criterion) {
    bench_generator_sequential_async_tokio::<_, SnowflakeTwitterId, _>(
        c,
        "mono/sequential/async/tokio/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a single `AtomicSnowflakeGenerator` using
/// `MonotonicClock` for tokio.
fn benchmark_mono_sequential_tokio_atomic(c: &mut Criterion) {
    bench_generator_sequential_async_tokio::<_, SnowflakeTwitterId, _>(
        c,
        "mono/sequential/async/tokio/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a single `LockSnowflakeGenerator` using `MonotonicClock`
/// for smol.
fn benchmark_mono_sequential_smol_lock(c: &mut Criterion) {
    bench_generator_sequential_async_smol::<_, SnowflakeTwitterId, _>(
        c,
        "mono/sequential/async/smol/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a single `AtomicSnowflakeGenerator` using
/// `MonotonicClock` for smol.
fn benchmark_mono_sequential_smol_atomic(c: &mut Criterion) {
    bench_generator_sequential_async_smol::<_, SnowflakeTwitterId, _>(
        c,
        "mono/sequential/async/smol/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `LockSnowflakeGenerator`s distributed across
/// tokio tasks.
fn benchmark_mono_tokio_lock(c: &mut Criterion) {
    bench_generator_async_tokio::<_, SnowflakeTwitterId, _>(
        c,
        "mono/multi/async/tokio/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `AtomicSnowflakeGenerator`s distributed across
/// tokio tasks.
fn benchmark_mono_tokio_atomic(c: &mut Criterion) {
    bench_generator_async_tokio::<_, SnowflakeTwitterId, _>(
        c,
        "mono/multi/async/tokio/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `LockSnowflakeGenerator`s distributed across
/// smol tasks.
fn benchmark_mono_smol_lock(c: &mut Criterion) {
    bench_generator_async_smol::<_, SnowflakeTwitterId, _>(
        c,
        "mono/multi/async/smol/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `AtomicSnowflakeGenerator`s distributed across
/// smol tasks.
fn benchmark_mono_smol_atomic(c: &mut Criterion) {
    bench_generator_async_smol::<_, SnowflakeTwitterId, _>(
        c,
        "mono/multi/async/smol/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

criterion_group!(
    benches,
    //Mock clock
    benchmark_mock_sequential_basic,
    benchmark_mock_sequential_lock,
    benchmark_mock_sequential_atomic,
    benchmark_mock_contended_lock,
    benchmark_mock_contended_atomic, // yields because of CAS failures
    // Monotonic clocks (yielding)
    benchmark_mono_sequential_basic,
    benchmark_mono_sequential_lock,
    benchmark_mono_sequential_atomic,
    benchmark_mono_threaded_lock,
    benchmark_mono_threaded_atomic,
    // Async single worker, single generator
    benchmark_mono_sequential_tokio_lock,
    benchmark_mono_sequential_tokio_atomic,
    benchmark_mono_sequential_smol_lock,
    benchmark_mono_sequential_smol_atomic,
    // Async multi worker, multi generator
    benchmark_mono_tokio_lock,
    benchmark_mono_tokio_atomic,
    benchmark_mono_smol_lock,
    benchmark_mono_smol_atomic,
);
criterion_main!(benches);
