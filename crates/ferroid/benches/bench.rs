use core::hint::black_box;
use core::time::Duration;
use criterion::async_executor::SmolExecutor;
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ferroid::{
    AtomicSnowflakeGenerator, Base32SnowExt, Base32UlidExt, BasicSnowflakeGenerator,
    BasicUlidGenerator, BeBytes, Id, IdGenStatus, LockSnowflakeGenerator, LockUlidGenerator,
    MonotonicClock, RandSource, SmolSleep, Snowflake, SnowflakeGenerator,
    SnowflakeGeneratorAsyncExt, SnowflakeMastodonId, SnowflakeTwitterId, ThreadRandom, TimeSource,
    ToU64, TokioSleep, ULID, Ulid, UlidGenerator, UlidGeneratorAsyncExt, UlidMono,
};
use futures::future::try_join_all;
use std::{thread::scope, time::Instant};
use tokio::runtime::Builder;

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
        u128::from(self.millis)
    }
}

// Number of IDs generated per benchmark iteration (per-thread for
// multi-threaded).
const TOTAL_IDS: usize = 4096; // 2^12 bits for sequence ID 

/// Benchmarks a hot-path generator (never pending) by always creating a new
/// generator instance which resets the sequence
fn bench_generator_hot<ID, G, T>(
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

    group.bench_function(format!("elems/{TOTAL_IDS}"), |b| {
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
fn bench_generator_hot_yield<ID, G, T>(
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

    group.bench_function(format!("elems/{TOTAL_IDS}"), |b| {
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

/// Benchmarks a generator per thread, with little to no yielding by aligning
/// the sequence value with each task
fn bench_generator_threaded<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(ID::Ty, T) -> G + Copy + Send + 'static,
    clock_factory: impl Fn() -> T + Copy + Send + 'static,
) where
    ID: Snowflake,
    ID::Ty: From<u64>,
    G: SnowflakeGenerator<ID, T>,
    T: TimeSource<ID::Ty> + Clone + Send + 'static,
{
    let mut group = c.benchmark_group(group_name);
    for thread_count in [1, 2, 4, 8, 16] {
        let total_ids = TOTAL_IDS * thread_count;
        group.throughput(Throughput::Elements(total_ids as u64));
        group.bench_function(format!("elems/{total_ids}/threads/{thread_count}"), |b| {
            b.iter_custom(|iters| {
                let clock = clock_factory();

                let start = Instant::now();
                for _ in 0..iters {
                    scope(|s| {
                        for i in 0..thread_count {
                            let clock = clock.clone();
                            s.spawn(move || {
                                // generator per thread
                                let generator = generator_fn(ID::Ty::from(i as u64), clock);
                                for _ in 0..TOTAL_IDS {
                                    loop {
                                        match generator.next_id() {
                                            IdGenStatus::Ready { id } => {
                                                black_box(id);
                                                break;
                                            }
                                            IdGenStatus::Pending { yield_for } => {
                                                std::thread::sleep(Duration::from_millis(
                                                    yield_for.to_u64(),
                                                ));
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    });
                }
                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Benchmarks many async generators in parallel, each running in a separate
/// task.
fn bench_generator_async_tokio<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G + Copy + Send + 'static,
    clock_factory: impl Fn() -> T + Copy,
) where
    ID: Snowflake + Send + Sync + 'static,
    G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
    G::Err: Send + Sync + 'static,
    T: TimeSource<ID::Ty> + Clone + Send + Sync + 'static,
{
    let mut group = c.benchmark_group(group_name);
    group.sample_size(10);
    group.sampling_mode(criterion::SamplingMode::Flat);

    for num_generators in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let total_ids = TOTAL_IDS * num_generators;

        group.throughput(Throughput::Elements(total_ids as u64));
        group.bench_function(format!("elems/{total_ids}/gens/{num_generators}"), |b| {
            let rt = Builder::new_multi_thread().enable_all().build().unwrap();

            b.to_async(&rt).iter_custom(move |iters| async move {
                let clock = clock_factory();

                let start = Instant::now();
                for _ in 0..iters {
                    let tasks = (0..num_generators).map(|i| {
                        let clock = clock.clone();
                        tokio::spawn(async move {
                            // generator per task
                            let generator = generator_fn(i as u64, clock.clone());
                            for _ in 0..TOTAL_IDS {
                                let id = generator.try_next_id_async::<TokioSleep>().await?;
                                black_box(id);
                            }
                            Ok::<(), G::Err>(())
                        })
                    });
                    try_join_all(tasks).await.unwrap();
                }
                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Benchmarks many async generators in parallel, each running in a separate
/// `smol` task.
///
/// # Panics
/// - If a task fails
pub fn bench_generator_async_smol<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G + Copy + Send + 'static,
    clock_factory: impl Fn() -> T + Copy,
) where
    ID: Snowflake + Send + Sync + 'static,
    G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
    G::Err: Send + Sync + 'static,
    T: TimeSource<ID::Ty> + Clone + Send + Sync + 'static,
{
    // Use all CPUs
    unsafe { std::env::set_var("SMOL_THREADS", num_cpus::get().to_string()) };

    let mut group = c.benchmark_group(group_name);
    group.sample_size(10);
    group.sampling_mode(criterion::SamplingMode::Flat);

    for num_generators in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let total_ids = TOTAL_IDS * num_generators;

        group.throughput(Throughput::Elements(total_ids as u64));
        group.bench_function(format!("elems/{total_ids}/gens/{num_generators}"), |b| {
            b.to_async(SmolExecutor).iter_custom(|iters| async move {
                let clock = clock_factory();

                let start = Instant::now();
                for _ in 0..iters {
                    let tasks = (0..num_generators).map(|i| {
                        let clock = clock.clone();
                        smol::spawn(async move {
                            // generator per task
                            let generator = generator_fn(i as u64, clock.clone());
                            for _ in 0..TOTAL_IDS {
                                let id = generator.try_next_id_async::<SmolSleep>().await?;
                                black_box(id);
                            }
                            Ok::<(), G::Err>(())
                        })
                    });
                    try_join_all(tasks).await.unwrap();
                }
                start.elapsed()
            });
        });
    }

    group.finish();
}

fn bench_generator_ulid<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_factory: impl Fn() -> G,
) where
    ID: Ulid,
    G: UlidGenerator<ID, T, R>,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(TOTAL_IDS as u64));

    group.bench_function(format!("elems/{TOTAL_IDS}"), |b| {
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

/// Benchmarks shared generator across threads, with yielding
fn bench_generator_ulid_threaded<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(T, R) -> G + Copy + Send + 'static,
    clock_factory: impl Fn() -> T + Copy + Send + 'static,
    rand_factory: impl Fn() -> R + Copy + Send + 'static,
) where
    ID: Ulid,
    G: UlidGenerator<ID, T, R>,
    T: TimeSource<ID::Ty> + Clone + Send + 'static,
    R: RandSource<ID::Ty> + Clone + Send + 'static,
{
    let mut group = c.benchmark_group(group_name);

    for thread_count in [1, 2, 4, 8, 16] {
        let total_ids = TOTAL_IDS * thread_count;
        // let ids_per_thread = total_ids / thread_count;
        group.throughput(Throughput::Elements(total_ids as u64));
        group.bench_function(format!("elems/{total_ids}/threads/{thread_count}"), |b| {
            b.iter_custom(|iters| {
                let clock = clock_factory();
                let rand = rand_factory();

                let start = Instant::now();
                for _ in 0..iters {
                    scope(|s| {
                        for _ in 0..thread_count {
                            let clock = clock.clone();
                            let rand = rand.clone();
                            s.spawn(move || {
                                // generator per thread
                                let generator = generator_fn(clock, rand);
                                for _ in 0..TOTAL_IDS {
                                    loop {
                                        match generator.next_id() {
                                            IdGenStatus::Ready { id } => {
                                                black_box(id);
                                                break;
                                            }
                                            IdGenStatus::Pending { yield_for } => {
                                                std::thread::sleep(Duration::from_millis(
                                                    yield_for.to_u64(),
                                                ));
                                            }
                                        }
                                    }
                                }
                            });
                        }
                    });
                }
                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Benchmarks many async generators in parallel, each running in a separate
/// task.
fn bench_ulid_generator_async_tokio<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(T, R) -> G + Copy + Send + 'static,
    clock_factory: impl Fn() -> T + Copy,
    rand_factory: impl Fn() -> R + Copy,
) where
    ID: Ulid + Send + Sync + 'static,
    G: UlidGenerator<ID, T, R> + Send + Sync + 'static,
    G::Err: Send + Sync + 'static,
    T: TimeSource<ID::Ty> + Clone + Send + Sync + 'static,
    R: RandSource<ID::Ty> + Clone + Send + Sync + 'static,
{
    let mut group = c.benchmark_group(group_name);
    group.sample_size(10);
    group.sampling_mode(criterion::SamplingMode::Flat);

    for num_generators in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let total_ids = TOTAL_IDS * num_generators;

        group.throughput(Throughput::Elements(total_ids as u64));
        group.bench_function(format!("elems/{total_ids}/gens/{num_generators}"), |b| {
            let rt = Builder::new_multi_thread().enable_all().build().unwrap();

            b.to_async(&rt).iter_custom(move |iters| async move {
                let clock = clock_factory();
                let rand = rand_factory();

                let start = Instant::now();
                for _ in 0..iters {
                    let tasks = (0..num_generators).map(|_| {
                        let clock = clock.clone();
                        let rand = rand.clone();
                        tokio::spawn(async move {
                            // generator per task
                            let generator = generator_fn(clock.clone(), rand.clone());
                            for _ in 0..TOTAL_IDS {
                                let id = generator.try_next_id_async::<TokioSleep>().await?;
                                black_box(id);
                            }
                            Ok::<(), G::Err>(())
                        })
                    });
                    try_join_all(tasks).await.unwrap();
                }
                start.elapsed()
            });
        });
    }

    group.finish();
}

/// Benchmarks many async generators in parallel, each running in a separate
/// task.
fn bench_ulid_generator_async_smol<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(T, R) -> G + Copy + Send + 'static,
    clock_factory: impl Fn() -> T + Copy,
    rand_factory: impl Fn() -> R + Copy,
) where
    ID: Ulid + Send + Sync + 'static,
    G: UlidGenerator<ID, T, R> + Send + Sync + 'static,
    G::Err: Send + Sync + 'static,
    T: TimeSource<ID::Ty> + Clone + Send + Sync + 'static,
    R: RandSource<ID::Ty> + Clone + Send + Sync + 'static,
{
    // Use all CPUs
    unsafe { std::env::set_var("SMOL_THREADS", num_cpus::get().to_string()) };

    let mut group = c.benchmark_group(group_name);
    group.sample_size(10);
    group.sampling_mode(criterion::SamplingMode::Flat);

    for num_generators in [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024] {
        let total_ids = TOTAL_IDS * num_generators;

        group.throughput(Throughput::Elements(total_ids as u64));
        group.bench_function(format!("elems/{total_ids}/gens/{num_generators}"), |b| {
            b.to_async(SmolExecutor).iter_custom(|iters| async move {
                let clock = clock_factory();
                let rand = rand_factory();

                let start = Instant::now();
                for _ in 0..iters {
                    let tasks = (0..num_generators).map(|_| {
                        let clock = clock.clone();
                        let rand = rand.clone();
                        smol::spawn(async move {
                            // generator per task
                            let generator = generator_fn(clock.clone(), rand.clone());
                            for _ in 0..TOTAL_IDS {
                                let id = generator.try_next_id_async::<SmolSleep>().await?;
                                black_box(id);
                            }
                            Ok::<(), G::Err>(())
                        })
                    });
                    try_join_all(tasks).await.unwrap();
                }
                start.elapsed()
            });
        });
    }

    group.finish();
}

fn bench_snow_base32<ID>(c: &mut Criterion, group_name: &str)
where
    ID: Snowflake + Base32SnowExt,
    ID::Ty: BeBytes,
{
    let id = ID::from_components(
        ID::max_timestamp(),
        ID::max_machine_id(),
        ID::max_sequence(),
    );
    let mut buf = ID::buf();

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));

    group.bench_function("encode/to_string", |b| {
        b.iter(|| {
            black_box(id.encode().to_string());
        });
    });
    group.bench_function("encode/as_str", |b| {
        b.iter(|| {
            black_box(id.encode().as_str());
        });
    });
    group.bench_function("encode/format", |b| {
        b.iter(|| {
            black_box(format!("{}", id.encode()));
        });
    });
    group.bench_function("encode/buffer/to_string", |b| {
        b.iter(|| {
            let b = id.encode_to_buf(black_box(&mut buf));
            black_box(b.to_string());
        });
    });
    group.bench_function("encode/buffer/as_str", |b| {
        b.iter(|| {
            let b = id.encode_to_buf(black_box(&mut buf));
            black_box(b.as_str());
        });
    });
    group.bench_function("encode/buffer/format", |b| {
        b.iter(|| {
            let b = id.encode_to_buf(black_box(&mut buf));
            black_box(format!("{b}"));
        });
    });

    let encoded = id.encode();
    let encoded_str = encoded.as_str();
    let encoded_string = encoded.to_string();
    group.bench_function("decode/as_ref", |b| {
        b.iter(|| {
            black_box(ID::decode(black_box(encoded.as_ref())).unwrap());
        });
    });
    group.bench_function("decode/str", |b| {
        b.iter(|| {
            black_box(ID::decode(black_box(encoded_str)).unwrap());
        });
    });
    group.bench_function("decode/string", |b| {
        b.iter(|| {
            black_box(ID::decode(black_box(&encoded_string)).unwrap());
        });
    });

    group.finish();
}

fn bench_ulid_base32<ID>(c: &mut Criterion, group_name: &str)
where
    ID: Ulid + Base32UlidExt,
    ID::Ty: BeBytes,
{
    let id = ID::from_components(ID::max_timestamp(), ID::max_random());
    let mut buf = ID::buf();

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));

    group.bench_function("encode/to_string", |b| {
        b.iter(|| {
            black_box(id.encode().to_string());
        });
    });
    group.bench_function("encode/as_str", |b| {
        b.iter(|| {
            black_box(id.encode().as_str());
        });
    });
    group.bench_function("encode/format", |b| {
        b.iter(|| {
            black_box(format!("{}", id.encode()));
        });
    });
    group.bench_function("encode/buffer/to_string", |b| {
        b.iter(|| {
            let b = id.encode_to_buf(black_box(&mut buf));
            black_box(b.to_string());
        });
    });
    group.bench_function("encode/buffer/as_str", |b| {
        b.iter(|| {
            let b = id.encode_to_buf(black_box(&mut buf));
            black_box(b.as_str());
        });
    });
    group.bench_function("encode/buffer/format", |b| {
        b.iter(|| {
            let b = id.encode_to_buf(black_box(&mut buf));
            black_box(format!("{b}"));
        });
    });

    let encoded = id.encode();
    let encoded_str = encoded.as_str();
    let encoded_string = encoded.to_string();
    group.bench_function("decode/as_ref", |b| {
        b.iter(|| {
            black_box(ID::decode(black_box(encoded.as_ref())).unwrap());
        });
    });
    group.bench_function("decode/str", |b| {
        b.iter(|| {
            black_box(ID::decode(black_box(encoded_str)).unwrap());
        });
    });
    group.bench_function("decode/string", |b| {
        b.iter(|| {
            black_box(ID::decode(black_box(&encoded_string)).unwrap());
        });
    });

    group.finish();
}

fn bench_clock<ID, T>(c: &mut Criterion, group_name: &str, clock_factory: impl Fn() -> T)
where
    ID: Id,
    ID::Ty: BeBytes,
    T: TimeSource<ID::Ty>,
{
    let clock = clock_factory();
    let mut group = c.benchmark_group(group_name);
    group.bench_function("current_millis", |b| {
        b.iter(|| {
            black_box(clock.current_millis());
        });
    });

    group.finish();
}

fn bench_rand<ID, R>(c: &mut Criterion, group_name: &str, rand_factory: impl Fn() -> R)
where
    ID: Id,
    ID::Ty: BeBytes,
    R: RandSource<ID::Ty>,
{
    let rand = rand_factory();
    let mut group = c.benchmark_group(group_name);
    group.bench_function("rand", |b| {
        b.iter(|| {
            black_box(rand.rand());
        });
    });

    group.finish();
}

fn bench_thread_local_ulid(c: &mut Criterion, group_name: &str) {
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("new_ulid", |b| {
        b.iter(|| {
            black_box(UlidMono::new_ulid());
        });
    });
    group.bench_function("from_timestamp", |b| {
        b.iter(|| {
            black_box(UlidMono::from_timestamp(42));
        });
    });
    let now = std::time::SystemTime::now();
    group.bench_function("from_datetime", |b| {
        b.iter(|| {
            black_box(UlidMono::from_datetime(now));
        });
    });

    group.finish();
}

pub fn bench_thread_local_ulid_threaded(c: &mut Criterion, group_name: &str) {
    let mut group = c.benchmark_group(group_name);

    for &thread_count in &[1, 2, 4, 8, 16] {
        let total_ids = TOTAL_IDS * thread_count;
        group.throughput(Throughput::Elements(total_ids as u64));

        group.bench_function(format!("elems/{total_ids}/threads/{thread_count}"), |b| {
            b.iter_custom(|iters| {
                let start = Instant::now();

                for _ in 0..iters {
                    scope(|s| {
                        for _ in 0..thread_count {
                            s.spawn(|| {
                                for _ in 0..TOTAL_IDS {
                                    black_box(UlidMono::new_ulid());
                                }
                            });
                        }
                    });
                }

                start.elapsed()
            });
        });
    }

    group.finish();
}

// --- MOCK CLOCK (fixed, non-advancing time) ---

/// Single-threaded benchmark for `BasicSnowflakeGenerator` with a fixed clock.
/// Always returns `Ready` (no yielding).
fn benchmark_mock_sequential_basic(c: &mut Criterion) {
    bench_generator_hot::<SnowflakeTwitterId, _, _>(c, "mock/sequential/basic", || {
        BasicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Single-threaded benchmark for `LockSnowflakeGenerator` with a fixed clock.
fn benchmark_mock_sequential_lock(c: &mut Criterion) {
    bench_generator_hot::<SnowflakeTwitterId, _, _>(c, "mock/sequential/lock", || {
        LockSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

/// Single-threaded benchmark for `AtomicSnowflakeGenerator` with a fixed clock.
fn benchmark_mock_sequential_atomic(c: &mut Criterion) {
    bench_generator_hot::<SnowflakeTwitterId, _, _>(c, "mock/sequential/atomic", || {
        AtomicSnowflakeGenerator::new(0, FixedMockTime { millis: 1 })
    });
}

// --- MONOTONIC CLOCK (realistic time with potential yielding) ---

/// Single-threaded benchmark for `BasicSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_sequential_basic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_hot_yield::<SnowflakeTwitterId, _, _>(c, "mono/sequential/basic", || {
        BasicSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Single-threaded benchmark for `LockSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_sequential_lock(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_hot_yield::<SnowflakeTwitterId, _, _>(c, "mono/sequential/lock", || {
        LockSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Single-threaded benchmark for `AtomicSnowflakeGenerator` with
/// `MonotonicClock`.
fn benchmark_mono_sequential_atomic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    bench_generator_hot_yield::<SnowflakeTwitterId, _, _>(c, "mono/sequential/atomic", || {
        AtomicSnowflakeGenerator::new(0, clock.clone())
    });
}

/// Multi-threaded benchmark for `BasicSnowflakeGenerator` with
/// `MonotonicClock`.
fn bench_generator_threaded_basic(c: &mut Criterion) {
    bench_generator_threaded::<SnowflakeTwitterId, _, _>(
        c,
        "mono/threaded/basic",
        BasicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}
/// Multi-threaded benchmark for `LockSnowflakeGenerator` with `MonotonicClock`.
fn bench_generator_threaded_lock(c: &mut Criterion) {
    bench_generator_threaded::<SnowflakeTwitterId, _, _>(
        c,
        "mono/threaded/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}
/// Multi-threaded benchmark for `AtomicSnowflakeGenerator` with
/// `MonotonicClock`.
fn bench_generator_threaded_atomic(c: &mut Criterion) {
    bench_generator_threaded::<SnowflakeTwitterId, _, _>(
        c,
        "mono/threaded/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

// --- ASYNC ---

/// Async benchmark for a pool of `LockSnowflakeGenerator`s distributed across
/// tokio tasks.
fn benchmark_mono_tokio_lock(c: &mut Criterion) {
    bench_generator_async_tokio::<SnowflakeTwitterId, _, _>(
        c,
        "mono/async/tokio/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `AtomicSnowflakeGenerator`s distributed across
/// tokio tasks.
fn benchmark_mono_tokio_atomic(c: &mut Criterion) {
    bench_generator_async_tokio::<SnowflakeTwitterId, _, _>(
        c,
        "mono/async/tokio/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `LockSnowflakeGenerator`s distributed across
/// smol tasks.
fn benchmark_mono_smol_lock(c: &mut Criterion) {
    bench_generator_async_smol::<SnowflakeTwitterId, _, _>(
        c,
        "mono/async/smol/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

/// Async benchmark for a pool of `AtomicSnowflakeGenerator`s distributed across
/// smol tasks.
fn benchmark_mono_smol_atomic(c: &mut Criterion) {
    bench_generator_async_smol::<SnowflakeTwitterId, _, _>(
        c,
        "mono/async/smol/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

// --- Ulid --- Mocks
fn benchmark_mock_sequential_ulid_basic(c: &mut Criterion) {
    let rand = ThreadRandom;
    bench_generator_ulid::<ULID, _, _, _>(c, "mock/sequential/ulid/basic", || {
        BasicUlidGenerator::new(FixedMockTime { millis: 1 }, rand.clone())
    });
}
fn benchmark_mock_sequential_ulid_lock(c: &mut Criterion) {
    let rand = ThreadRandom;
    bench_generator_ulid::<ULID, _, _, _>(c, "mock/sequential/ulid/lock", || {
        LockUlidGenerator::new(FixedMockTime { millis: 1 }, rand.clone())
    });
}
// Mono clocks
fn benchmark_mono_sequential_ulid_basic(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    let rand = ThreadRandom;
    bench_generator_ulid::<ULID, _, _, _>(c, "mono/sequential/ulid/basic", || {
        BasicUlidGenerator::new(clock.clone(), rand.clone())
    });
}
fn benchmark_mono_sequential_ulid_lock(c: &mut Criterion) {
    let clock = MonotonicClock::default();
    let rand = ThreadRandom;
    bench_generator_ulid::<ULID, _, _, _>(c, "mono/sequential/ulid/lock", || {
        LockUlidGenerator::new(clock.clone(), rand.clone())
    });
}
fn benchmark_mono_threaded_ulid_basic(c: &mut Criterion) {
    bench_generator_ulid_threaded::<ULID, _, _, _>(
        c,
        "mono/threaded/ulid/basic",
        BasicUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}
fn benchmark_mono_threaded_ulid_lock(c: &mut Criterion) {
    bench_generator_ulid_threaded::<ULID, _, _, _>(
        c,
        "mono/threaded/ulid/lock",
        LockUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}
// Ulid Async
fn benchmark_mono_tokio_ulid_lock(c: &mut Criterion) {
    bench_ulid_generator_async_tokio::<ULID, _, _, _>(
        c,
        "mono/async/tokio/ulid/lock",
        LockUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}
fn benchmark_mono_smol_ulid_lock(c: &mut Criterion) {
    bench_ulid_generator_async_smol::<ULID, _, _, _>(
        c,
        "mono/async/smol/ulid/lock",
        LockUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}

// Base32 encode/decode
fn bench_base32(c: &mut Criterion) {
    bench_snow_base32::<SnowflakeMastodonId>(c, "base32/snow");
    bench_ulid_base32::<ULID>(c, "base32/ulid");
}

fn bench_mono_clock(c: &mut Criterion) {
    bench_clock::<SnowflakeTwitterId, _>(c, "mono/clock/u64", MonotonicClock::default);
    bench_clock::<ULID, _>(c, "mono/clock/u128", MonotonicClock::default);
}
fn bench_thread_rand(c: &mut Criterion) {
    bench_rand::<SnowflakeTwitterId, _>(c, "thread/rng/u64", ThreadRandom::default);
    bench_rand::<ULID, _>(c, "thread/rng/u128", ThreadRandom::default);
}

fn bench_thread_local(c: &mut Criterion) {
    bench_thread_local_ulid(c, "thread_local/ulid");
    bench_thread_local_ulid_threaded(c, "thread_local/ulid");
}

criterion_group!(
    benches,
    // --- Base32 ---
    bench_base32,
    // --- Clock ---
    bench_mono_clock,
    // --- RNG ---
    bench_thread_rand,
    // --- Thread locals ---
    bench_thread_local,
    // --- Snowflake ---
    //
    // Mock clock
    benchmark_mock_sequential_basic,
    benchmark_mock_sequential_lock,
    benchmark_mock_sequential_atomic,
    // Monotonic clocks
    benchmark_mono_sequential_basic,
    benchmark_mono_sequential_lock,
    benchmark_mono_sequential_atomic,
    // Multithreaded (generator per thread)
    bench_generator_threaded_basic,
    bench_generator_threaded_lock,
    bench_generator_threaded_atomic,
    // Async multi worker, multi generator
    benchmark_mono_tokio_lock,
    benchmark_mono_tokio_atomic,
    benchmark_mono_smol_lock,
    benchmark_mono_smol_atomic,
    // --- Ulid ---
    //
    // Mock clock
    benchmark_mock_sequential_ulid_basic,
    benchmark_mock_sequential_ulid_lock,
    // Monotonic clocks
    benchmark_mono_sequential_ulid_basic,
    benchmark_mono_sequential_ulid_lock,
    // Multithreaded (generator per thread)
    benchmark_mono_threaded_ulid_basic,
    benchmark_mono_threaded_ulid_lock,
    // Async multi worker, multi generator
    benchmark_mono_tokio_ulid_lock,
    benchmark_mono_smol_ulid_lock,
);
criterion_main!(benches);
