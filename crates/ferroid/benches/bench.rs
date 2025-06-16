use async_lock::Barrier as SmolBarrier;
use core::hint::black_box;
use criterion::async_executor::SmolExecutor;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ferroid::{
    AtomicSnowflakeGenerator, BasicFluidGenerator, BasicSnowflakeGenerator, Error, Fluid,
    FluidGenerator, IdGenStatus, LockSnowflakeGenerator, MonotonicClock, RandSource, SmolSleep,
    Snowflake, SnowflakeGenerator, SnowflakeGeneratorAsyncExt, SnowflakeTwitterId, ThreadRandom,
    TimeSource, TokioSleep, Ulid,
};
use futures::future::try_join_all;
use std::{
    sync::{Arc, Barrier},
    thread::scope,
    time::Instant,
};
use tokio::runtime::Builder;
use tokio::sync::Barrier as TokioBarrier;

struct FixedMockTime {
    millis: u64,
}

impl TimeSource<u64> for FixedMockTime {
    fn current_millis(&self) -> u64 {
        self.millis
    }
}

impl TimeSource<u128> for FixedMockTime {
    fn current_millis(&self) -> u128 {
        self.millis as u128
    }
}

// Number of IDs generated per benchmark iteration (per-thread for
// multi-threaded).
const TOTAL_IDS: usize = 4096;

/// Benchmarks a hot-path generator (never pending) by always creating a new
/// generator instance which resets the sequence
fn bench_generator<ID, G, T>(c: &mut Criterion, group_name: &str, generator_factory: impl Fn() -> G)
where
    ID: Snowflake,
    G: SnowflakeGenerator<ID, T>,
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

/// Benchmarks a hot-path generator (spinloop on pending) by always creating a
/// new generator instance which resets the sequence
fn bench_generator_yield<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_factory: impl Fn() -> G,
) where
    ID: Snowflake,
    G: SnowflakeGenerator<ID, T>,
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

/// Benchmarks many async generators in parallel, each running in a separate
/// task.
fn bench_generator_async_tokio<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G + Copy,
    clock_factory: impl Fn() -> T + Copy,
) where
    ID: Snowflake + Send + Sync + 'static,
    G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
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
                    let barrier = Arc::new(TokioBarrier::new(num_generators + 1));
                    let done = Arc::new(TokioBarrier::new(num_generators + 1));

                    let tasks: Vec<_> = (0..num_generators)
                        .map(|i| {
                            let generator = generator_fn(i as u64, clock.clone());
                            let barrier = Arc::clone(&barrier);
                            let done = Arc::clone(&done);

                            tokio::spawn(async move {
                                for _ in 0..iters {
                                    // Wait for start signal
                                    barrier.wait().await;

                                    for _ in 0..ids_per_task {
                                        let id =
                                            generator.try_next_id_async::<TokioSleep>().await?;
                                        black_box(id);
                                    }

                                    // Signal completion
                                    done.wait().await;
                                }
                                Ok::<(), Error>(())
                            })
                        })
                        .collect();

                    let start = Instant::now();
                    for _ in 0..iters {
                        TokioBarrier::wait(&barrier).await;
                        TokioBarrier::wait(&done).await;
                    }

                    try_join_all(tasks).await.unwrap();

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Benchmarks many async generators in parallel, each running in a separate
/// `smol` task.
pub fn bench_generator_async_smol<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G + Copy,
    clock_factory: impl Fn() -> T + Copy,
) where
    ID: Snowflake + Send,
    G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
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
                    let barrier = Arc::new(SmolBarrier::new(num_generators + 1));
                    let done = Arc::new(SmolBarrier::new(num_generators + 1));

                    let tasks: Vec<_> = (0..num_generators)
                        .map(|i| {
                            let generator = generator_fn(i as u64, clock.clone());
                            let barrier = Arc::clone(&barrier);
                            let done = Arc::clone(&done);

                            smol::spawn(async move {
                                for _ in 0..iters {
                                    // Wait for start signal
                                    barrier.wait().await;

                                    for _ in 0..ids_per_task {
                                        let id = generator.try_next_id_async::<SmolSleep>().await?;
                                        black_box(id);
                                    }

                                    // Signal completion
                                    done.wait().await;
                                }
                                Ok::<(), Error>(())
                            })
                        })
                        .collect();

                    let start = Instant::now();
                    for _ in 0..iters {
                        SmolBarrier::wait(&barrier).await;
                        SmolBarrier::wait(&done).await;
                    }

                    try_join_all(tasks).await.unwrap();

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

fn bench_generator_fluid<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_factory: impl Fn() -> G,
) where
    ID: Fluid,
    G: FluidGenerator<ID, T, R>,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(TOTAL_IDS as u64));

    group.bench_function(format!("elems/{}", TOTAL_IDS), |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            let generator = generator_factory();

            for _ in 0..iters {
                for _ in 0..TOTAL_IDS {
                    let id = generator.next_id();
                    black_box(id);
                }
            }

            start.elapsed()
        });
    });

    group.finish();
}

/// Benchmarks shared generator across threads, with no yielding (fixed clock).
fn bench_generator_fluid_contended<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn() -> G,
) where
    ID: Fluid,
    G: FluidGenerator<ID, T, R> + Send + Sync,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);

    for thread_count in [1, 2, 4, 8, 16] {
        let ids_per_thread = TOTAL_IDS / thread_count;

        group.throughput(Throughput::Elements(TOTAL_IDS as u64));
        group.bench_function(
            format!("elems/{}/threads/{}", TOTAL_IDS, thread_count),
            |b| {
                b.iter_custom(|iters| {
                    let generator = Arc::new(generator_fn());
                    let barrier = Arc::new(Barrier::new(thread_count + 1));

                    scope(|s| {
                        for _ in 0..thread_count {
                            let generator = Arc::clone(&generator);
                            let barrier = Arc::clone(&barrier);

                            s.spawn(move || {
                                for _ in 0..iters {
                                    barrier.wait();

                                    for _ in 0..ids_per_thread {
                                        let id = generator.next_id();
                                        black_box(id);
                                    }
                                }
                            });
                        }

                        let start = Instant::now();

                        for _ in 0..iters {
                            barrier.wait();
                        }

                        start.elapsed()
                    })
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
    bench_generator::<SnowflakeTwitterId, _, _>(c, "mock/sequential/basic", || {
        BasicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Single-threaded benchmark for `LockSnowflakeGenerator` with a fixed clock.
fn benchmark_mock_sequential_lock(c: &mut Criterion) {
    bench_generator::<SnowflakeTwitterId, _, _>(c, "mock/sequential/lock", || {
        LockSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Single-threaded benchmark for `AtomicSnowflakeGenerator` with a fixed clock.
fn benchmark_mock_sequential_atomic(c: &mut Criterion) {
    bench_generator::<SnowflakeTwitterId, _, _>(c, "mock/sequential/atomic", || {
        AtomicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

// --- MONOTONIC CLOCK (realistic time with potential yielding) ---

/// Single-threaded benchmark for `BasicSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_sequential_basic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_yield::<SnowflakeTwitterId, _, _>(c, "mono/sequential/basic", || {
        BasicSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Single-threaded benchmark for `LockSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_sequential_lock(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_yield::<SnowflakeTwitterId, _, _>(c, "mono/sequential/lock", || {
        LockSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Single-threaded benchmark for `AtomicSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_sequential_atomic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_yield::<SnowflakeTwitterId, _, _>(c, "mono/sequential/atomic", || {
        AtomicSnowflakeGenerator::new(0, clock.clone())
    });
}

// --- ASYNC ---

/// Async benchmark for a pool of `LockSnowflakeGenerator`s distributed across
/// tokio tasks.
fn benchmark_mono_tokio_lock(c: &mut Criterion) {
    bench_generator_async_tokio::<SnowflakeTwitterId, _, _>(
        c,
        "mono/multi/async/tokio/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `AtomicSnowflakeGenerator`s distributed across
/// tokio tasks.
fn benchmark_mono_tokio_atomic(c: &mut Criterion) {
    bench_generator_async_tokio::<SnowflakeTwitterId, _, _>(
        c,
        "mono/multi/async/tokio/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `LockSnowflakeGenerator`s distributed across
/// smol tasks.
fn benchmark_mono_smol_lock(c: &mut Criterion) {
    bench_generator_async_smol::<SnowflakeTwitterId, _, _>(
        c,
        "mono/multi/async/smol/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `AtomicSnowflakeGenerator`s distributed across
/// smol tasks.
fn benchmark_mono_smol_atomic(c: &mut Criterion) {
    bench_generator_async_smol::<SnowflakeTwitterId, _, _>(
        c,
        "mono/multi/async/smol/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

// --- Fluid ---
/// Single-threaded benchmark for `BasicFluidGenerator` with a fixed clock.
fn benchmark_mono_sequential_fluid(c: &mut Criterion) {
    bench_generator_fluid::<Ulid, _, _, _>(c, "mock/sequential/fluid", || {
        BasicFluidGenerator::new(FixedMockTime { millis: 1 }, ThreadRandom::default())
    });
}

fn benchmark_mono_contended_fluid(c: &mut Criterion) {
    bench_generator_fluid_contended::<Ulid, _, _, _>(c, "mock/contended/fluid", || {
        BasicFluidGenerator::new(FixedMockTime { millis: 1 }, ThreadRandom::default())
    });
}

criterion_group!(
    benches,
    // Mock clock (raw throughput in hot path)
    benchmark_mock_sequential_basic,
    benchmark_mock_sequential_lock,
    benchmark_mock_sequential_atomic,
    // Monotonic clocks (yielding, raw throughput in hot path)
    benchmark_mono_sequential_basic,
    benchmark_mono_sequential_lock,
    benchmark_mono_sequential_atomic,
    // Async multi worker, multi generator
    benchmark_mono_tokio_lock,
    benchmark_mono_tokio_atomic,
    benchmark_mono_smol_lock,
    benchmark_mono_smol_atomic,
    // Fluid
    benchmark_mono_sequential_fluid,
    benchmark_mono_contended_fluid,
);
criterion_main!(benches);
