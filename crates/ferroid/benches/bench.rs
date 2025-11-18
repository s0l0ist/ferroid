use core::{fmt, hint::black_box};
use std::time::Duration;

use criterion::{
    Criterion, Throughput, async_executor::SmolExecutor, criterion_group, criterion_main,
};
use ferroid::{
    base32::{Base32SnowExt, Base32UlidExt},
    futures::{SmolSleep, SnowflakeGeneratorAsyncExt, TokioSleep, UlidGeneratorAsyncExt},
    generator::{
        AtomicMonoUlidGenerator, AtomicSnowflakeGenerator, BasicMonoUlidGenerator,
        BasicSnowflakeGenerator, BasicUlidGenerator, IdGenStatus, LockMonoUlidGenerator,
        LockSnowflakeGenerator, SnowflakeGenerator, UlidGenerator, thread_local::Ulid,
    },
    id::{BeBytes, SnowflakeId, SnowflakeMastodonId, SnowflakeTwitterId, ULID, UlidId},
    rand::{RandSource, ThreadRandom},
    time::{MonotonicClock, TimeSource},
};
use tokio::runtime::Builder;

fn bench_snow_base32<ID>(c: &mut Criterion, group_name: &str)
where
    ID: SnowflakeId + Base32SnowExt + fmt::Display,
    ID::Ty: BeBytes,
{
    let id = ID::from_components(
        ID::max_timestamp(),
        ID::max_machine_id(),
        ID::max_sequence(),
    );

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(<ID::Ty as BeBytes>::SIZE as u64));
    group.bench_function("encode/as_string", |b| {
        b.iter(|| {
            black_box(id.encode().as_string());
        });
    });
    group.bench_function("encode/as_str", |b| {
        b.iter(|| {
            black_box(id.encode().as_str());
        });
    });

    let mut buf = ID::buf();
    group.bench_function("encode_to_buf", |b| {
        b.iter(|| {
            let b = id.encode_to_buf(&mut buf);
            black_box(b);
        });
    });

    group.throughput(Throughput::Bytes(<ID::Ty as BeBytes>::BASE32_SIZE as u64));
    let encoded = id.encode();
    group.bench_function("decode", |b| {
        b.iter(|| {
            black_box(ID::decode(encoded.as_ref()).unwrap());
        });
    });

    group.finish();
}

fn bench_ulid_base32<ID>(c: &mut Criterion, group_name: &str)
where
    ID: UlidId + Base32UlidExt + fmt::Display,
    ID::Ty: BeBytes,
{
    let id = ID::from_components(ID::max_timestamp(), ID::max_random());

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Bytes(<ID::Ty as BeBytes>::SIZE as u64));
    group.bench_function("encode/as_string", |b| {
        b.iter(|| {
            black_box(id.encode().as_string());
        });
    });
    group.bench_function("encode/as_str", |b| {
        b.iter(|| {
            black_box(id.encode().as_str());
        });
    });

    let mut buf = ID::buf();
    group.bench_function("encode_to_buf", |b| {
        b.iter(|| {
            let b = id.encode_to_buf(&mut buf);
            black_box(b);
        });
    });

    group.throughput(Throughput::Bytes(<ID::Ty as BeBytes>::BASE32_SIZE as u64));
    let encoded = id.encode();
    group.bench_function("decode", |b| {
        b.iter(|| {
            black_box(ID::decode(encoded.as_ref()).unwrap());
        });
    });

    group.finish();
}

fn bench_ulid_constructors(c: &mut Criterion, group_name: &str) {
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("now", |b| {
        b.iter(|| {
            black_box(ULID::now());
        });
    });
    group.bench_function("from", |b| {
        b.iter(|| {
            black_box(ULID::from(42, 42));
        });
    });
    group.bench_function("from_timestamp", |b| {
        b.iter(|| {
            black_box(ULID::from_timestamp(42));
        });
    });
    group.bench_function("from_datetime", |b| {
        let now = std::time::SystemTime::now();
        b.iter(|| {
            black_box(ULID::from_datetime(now));
        });
    });

    group.finish();
}

fn bench_snow_constructors(c: &mut Criterion, group_name: &str) {
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("from", |b| {
        b.iter(|| {
            black_box(SnowflakeTwitterId::from(42, 42, 42));
        });
    });
    group.finish();
}

fn bench_thread_local_ulid(c: &mut Criterion, group_name: &str) {
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("new_ulid", |b| {
        b.iter(|| {
            black_box(Ulid::new_ulid());
        });
    });
    group.bench_function("new_mono_ulid", |b| {
        b.iter(|| {
            black_box(Ulid::new_mono_ulid());
        });
    });
    group.bench_function("from_timestamp", |b| {
        b.iter(|| {
            black_box(Ulid::from_timestamp(42));
        });
    });
    group.bench_function("from_datetime", |b| {
        let now = std::time::SystemTime::now();
        b.iter(|| {
            black_box(Ulid::from_datetime(now));
        });
    });

    group.finish();
}

fn bench_generator_hot_yield<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(T) -> G,
    clock_fn: impl Fn() -> T,
) where
    ID: SnowflakeId,
    G: SnowflakeGenerator<ID, T>,
    T: TimeSource<ID::Ty> + Clone,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_id", |b| {
        let clock = clock_fn();
        b.iter(|| {
            let generator = generator_fn(clock.clone());
            loop {
                match generator.next_id() {
                    IdGenStatus::Ready { id } => {
                        black_box(id);
                        break;
                    }
                    IdGenStatus::Pending { .. } => core::hint::spin_loop(),
                }
            }
        });
    });

    group.finish();
}

fn bench_generator_ulid<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(T, R) -> G,
    clock_fn: impl Fn() -> T,
    rand_fn: impl Fn() -> R,
) where
    ID: UlidId,
    G: UlidGenerator<ID, T, R>,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_id", |b| {
        let clock = clock_fn();
        let rand = rand_fn();
        let generator = generator_fn(clock, rand);
        b.iter(|| {
            loop {
                match generator.next_id() {
                    IdGenStatus::Ready { id } => {
                        black_box(id);
                        break;
                    }
                    IdGenStatus::Pending { .. } => core::hint::spin_loop(),
                }
            }
        });
    });
    group.finish();
}

/// Benchmarks the latency of generating a single ID in an async context
fn bench_snow_generator_async_single_tokio<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G,
    clock_factory: impl Fn() -> T,
) where
    G: SnowflakeGenerator<ID, T> + Sync,
    ID: SnowflakeId + Send,
    T: TimeSource<ID::Ty> + Clone + Send,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_id", |b| {
        let rt = Builder::new_multi_thread().enable_all().build().unwrap();
        let clock = clock_factory();
        b.to_async(&rt).iter(|| async {
            let generator = generator_fn(0, clock.clone());
            let id = generator.try_next_id_async::<TokioSleep>().await.unwrap();
            black_box(id);
        });
    });

    group.finish();
}

/// Benchmarks the latency of generating a single ID in an async context (smol)
fn bench_snow_generator_async_single_smol<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G,
    clock_factory: impl Fn() -> T,
) where
    G: SnowflakeGenerator<ID, T> + Sync,
    ID: SnowflakeId + Send,
    T: TimeSource<ID::Ty> + Clone + Send,
{
    unsafe { std::env::set_var("SMOL_THREADS", num_cpus::get().to_string()) };

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_id", |b| {
        let clock = clock_factory();
        b.to_async(SmolExecutor).iter(|| async {
            let generator = generator_fn(0, clock.clone());
            let id = generator.try_next_id_async::<SmolSleep>().await.unwrap();
            black_box(id);
        });
    });

    group.finish();
}

/// Benchmarks the latency of generating a single ULID in an async context
/// (tokio)
fn bench_ulid_generator_async_single_tokio<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(T, R) -> G,
    clock_factory: impl Fn() -> T,
    rand_factory: impl Fn() -> R,
) where
    G: UlidGenerator<ID, T, R> + Sync,
    ID: UlidId + Send,
    T: TimeSource<ID::Ty> + Send,
    R: RandSource<ID::Ty> + Send,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_id", |b| {
        let rt = Builder::new_multi_thread().enable_all().build().unwrap();
        let clock = clock_factory();
        let rand = rand_factory();
        let generator = generator_fn(clock, rand);
        b.to_async(&rt).iter(|| async {
            let id = generator.try_next_id_async::<TokioSleep>().await.unwrap();
            black_box(id);
        });
    });

    group.finish();
}

/// Benchmarks the latency of generating a single ULID in an async context
/// (smol)
fn bench_ulid_generator_async_single_smol<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(T, R) -> G,
    clock_factory: impl Fn() -> T,
    rand_factory: impl Fn() -> R,
) where
    G: UlidGenerator<ID, T, R> + Sync,
    ID: UlidId + Send,
    T: TimeSource<ID::Ty> + Send,
    R: RandSource<ID::Ty> + Send,
{
    unsafe { std::env::set_var("SMOL_THREADS", num_cpus::get().to_string()) };

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_id", |b| {
        let clock = clock_factory();
        let rand = rand_factory();
        let generator = generator_fn(clock, rand);
        b.to_async(SmolExecutor).iter(|| async {
            let id = generator.try_next_id_async::<SmolSleep>().await.unwrap();
            black_box(id);
        });
    });

    group.finish();
}

// Base32 encode/decode
fn bench_base32(c: &mut Criterion) {
    bench_snow_base32::<SnowflakeMastodonId>(c, "base32/snow");
    bench_ulid_base32::<ULID>(c, "base32/ulid");
}

fn bench_constructors(c: &mut Criterion) {
    bench_snow_constructors(c, "SnowflakeTwitterId");
    bench_ulid_constructors(c, "ULID");
}

fn bench_thread_local(c: &mut Criterion) {
    bench_thread_local_ulid(c, "thread_local/ulid");
}

fn benchmark_mono_sequential_basic(c: &mut Criterion) {
    bench_generator_hot_yield::<SnowflakeTwitterId, _, _>(
        c,
        "mono/sequential/snow/basic",
        |time| BasicSnowflakeGenerator::new(0, time),
        MonotonicClock::default,
    );
}

fn benchmark_mono_sequential_lock(c: &mut Criterion) {
    bench_generator_hot_yield::<SnowflakeTwitterId, _, _>(
        c,
        "mono/sequential/snow/lock",
        |time| LockSnowflakeGenerator::new(0, time),
        MonotonicClock::default,
    );
}

fn benchmark_mono_sequential_atomic(c: &mut Criterion) {
    bench_generator_hot_yield::<SnowflakeTwitterId, _, _>(
        c,
        "mono/sequential/snow/atomic",
        |time| AtomicSnowflakeGenerator::new(0, time),
        MonotonicClock::default,
    );
}

fn benchmark_mono_sequential_ulid_basic(c: &mut Criterion) {
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "mono/sequential/ulid/basic",
        |time, rng| BasicUlidGenerator::new(time, rng),
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "mono/sequential/ulid/basic_mono",
        |time, rng| BasicMonoUlidGenerator::new(time, rng),
        MonotonicClock::default,
        ThreadRandom::default,
    );
}
fn benchmark_mono_sequential_ulid_lock(c: &mut Criterion) {
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "mono/sequential/ulid/lock_mono",
        |time, rng| LockMonoUlidGenerator::new(time, rng),
        MonotonicClock::default,
        ThreadRandom::default,
    );
}
fn benchmark_mono_sequential_ulid_atomic(c: &mut Criterion) {
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "mono/sequential/ulid/atomicmono",
        |time, rng| AtomicMonoUlidGenerator::new(time, rng),
        MonotonicClock::default,
        ThreadRandom::default,
    );
}

fn benchmark_async_single_tokio_lock(c: &mut Criterion) {
    bench_snow_generator_async_single_tokio::<SnowflakeTwitterId, _, _>(
        c,
        "async/tokio/snow/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

fn benchmark_async_single_tokio_atomic(c: &mut Criterion) {
    bench_snow_generator_async_single_tokio::<SnowflakeTwitterId, _, _>(
        c,
        "async/tokio/snow/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

fn benchmark_async_single_smol_lock(c: &mut Criterion) {
    bench_snow_generator_async_single_smol::<SnowflakeTwitterId, _, _>(
        c,
        "async/smol/snow/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

fn benchmark_async_single_smol_atomic(c: &mut Criterion) {
    bench_snow_generator_async_single_smol::<SnowflakeTwitterId, _, _>(
        c,
        "async/smol/snow/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

fn benchmark_async_single_tokio_ulid_lock(c: &mut Criterion) {
    bench_ulid_generator_async_single_tokio::<ULID, _, _, _>(
        c,
        "async/tokio/ulid/lock",
        LockMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}

fn benchmark_async_single_tokio_ulid_atomic(c: &mut Criterion) {
    bench_ulid_generator_async_single_tokio::<ULID, _, _, _>(
        c,
        "async/tokio/ulid/atomic",
        AtomicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}

fn benchmark_async_single_smol_ulid_lock(c: &mut Criterion) {
    bench_ulid_generator_async_single_smol::<ULID, _, _, _>(
        c,
        "async/smol/ulid/lock",
        LockMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}

fn benchmark_async_single_smol_ulid_atomic(c: &mut Criterion) {
    bench_ulid_generator_async_single_smol::<ULID, _, _, _>(
        c,
        "async/smol/ulid/atomic",
        AtomicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}

criterion_group!(
    name = benches;
    config = Criterion::default()
        .sample_size(100)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(1));
    targets =
        // --- Base32 ---
        bench_base32,
        // --- Constructors ---
        bench_constructors,
        // --- Thread locals ---
        bench_thread_local,
        // --- ULID Sequential ---
        benchmark_mono_sequential_ulid_basic,
        benchmark_mono_sequential_ulid_lock,
        benchmark_mono_sequential_ulid_atomic,
        // --- ULID Async ---
        benchmark_async_single_tokio_ulid_lock,
        benchmark_async_single_tokio_ulid_atomic,
        benchmark_async_single_smol_ulid_lock,
        benchmark_async_single_smol_ulid_atomic,
        // --- Snowflake Sequential ---
        benchmark_mono_sequential_basic,
        benchmark_mono_sequential_lock,
        benchmark_mono_sequential_atomic,
        // --- Snowflake Async ---
        benchmark_async_single_tokio_lock,
        benchmark_async_single_tokio_atomic,
        benchmark_async_single_smol_lock,
        benchmark_async_single_smol_atomic,

);
criterion_main!(benches);
