use core::{fmt, hint::black_box};
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
    id::{BeBytes, SnowflakeId, SnowflakeTwitterId, ULID, UlidId},
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
    group.bench_function("encode_to_buf", |b| {
        let mut buf = ID::buf();
        b.iter(|| {
            let b = id.encode_to_buf(&mut buf);
            black_box(b);
        });
    });
    group.throughput(Throughput::Bytes(<ID::Ty as BeBytes>::BASE32_SIZE as u64));
    group.bench_function("decode", |b| {
        let encoded = id.encode();
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
    group.bench_function("encode_to_buf", |b| {
        let mut buf = ID::buf();
        b.iter(|| {
            let b = id.encode_to_buf(&mut buf);
            black_box(b);
        });
    });
    group.throughput(Throughput::Bytes(<ID::Ty as BeBytes>::BASE32_SIZE as u64));
    group.bench_function("decode", |b| {
        let encoded = id.encode();
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
            black_box(ULID::from(41, 42));
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
            black_box(SnowflakeTwitterId::from(40, 41, 42));
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

fn bench_generator_snow<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G,
    clock_fn: impl Fn() -> T,
) where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty> + Clone,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_id", |b| {
        let clock = clock_fn();
        b.iter_batched(
            || generator_fn(0, clock.clone()),
            |generator| {
                loop {
                    match generator.next_id() {
                        IdGenStatus::Ready { id } => {
                            black_box(id);
                            break;
                        }
                        IdGenStatus::Pending { .. } => core::hint::spin_loop(),
                    }
                }
            },
            criterion::BatchSize::NumBatches(256),
        );
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
    G: UlidGenerator<ID, T, R>,
    ID: UlidId,
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
fn bench_async_snow_tokio<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G,
    clock_fn: impl Fn() -> T,
) where
    G: SnowflakeGenerator<ID, T> + Sync,
    ID: SnowflakeId + Send,
    T: TimeSource<ID::Ty> + Clone + Send,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_id", |b| {
        let rt = Builder::new_multi_thread().enable_all().build().unwrap();
        let clock = clock_fn();
        b.to_async(&rt).iter_batched(
            || generator_fn(0, clock.clone()),
            |generator| async move {
                let id = generator.try_next_id_async::<TokioSleep>().await.unwrap();
                black_box(id);
            },
            criterion::BatchSize::NumBatches(256),
        );
    });
    group.finish();
}

/// Benchmarks the latency of generating a single ID in an async context (smol)
fn bench_async_snow_smol<ID, G, T>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(u64, T) -> G,
    clock_fn: impl Fn() -> T,
) where
    G: SnowflakeGenerator<ID, T> + Sync,
    ID: SnowflakeId + Send,
    T: TimeSource<ID::Ty> + Clone + Send,
{
    unsafe { std::env::set_var("SMOL_THREADS", num_cpus::get().to_string()) };

    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("single_id", |b| {
        let clock = clock_fn();
        b.to_async(SmolExecutor).iter_batched(
            || generator_fn(0, clock.clone()),
            |generator| async move {
                let id = generator.try_next_id_async::<SmolSleep>().await.unwrap();
                black_box(id);
            },
            criterion::BatchSize::NumBatches(256),
        );
    });
    group.finish();
}

/// Benchmarks the latency of generating a single ULID in an async context
/// (tokio)
fn bench_async_ulid_tokio<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(T, R) -> G,
    clock_fn: impl Fn() -> T,
    rand_fn: impl Fn() -> R,
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
        let clock = clock_fn();
        let rand = rand_fn();
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
fn bench_async_ulid_smol<ID, G, T, R>(
    c: &mut Criterion,
    group_name: &str,
    generator_fn: impl Fn(T, R) -> G,
    clock_fn: impl Fn() -> T,
    rand_fn: impl Fn() -> R,
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
        let clock = clock_fn();
        let rand = rand_fn();
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
    bench_snow_base32::<SnowflakeTwitterId>(c, "base32/snow");
    bench_ulid_base32::<ULID>(c, "base32/ulid");
}

fn bench_constructors(c: &mut Criterion) {
    bench_snow_constructors(c, "SnowflakeTwitterId");
    bench_ulid_constructors(c, "ULID");
}

fn bench_thread_local(c: &mut Criterion) {
    bench_thread_local_ulid(c, "thread_local/ulid");
}

fn benchmark_snow(c: &mut Criterion) {
    bench_generator_snow::<SnowflakeTwitterId, _, _>(
        c,
        "mono/sequential/snow/basic",
        BasicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_generator_snow::<SnowflakeTwitterId, _, _>(
        c,
        "mono/sequential/snow/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_generator_snow::<SnowflakeTwitterId, _, _>(
        c,
        "mono/sequential/snow/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

fn benchmark_ulid(c: &mut Criterion) {
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "mono/sequential/ulid/basic",
        BasicUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "mono/sequential/ulid/basic_mono",
        BasicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "mono/sequential/ulid/lock_mono",
        LockMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "mono/sequential/ulid/atomicmono",
        AtomicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}

fn benchmark_async_snow(c: &mut Criterion) {
    bench_async_snow_tokio::<SnowflakeTwitterId, _, _>(
        c,
        "async/tokio/snow/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_async_snow_tokio::<SnowflakeTwitterId, _, _>(
        c,
        "async/tokio/snow/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_async_snow_smol::<SnowflakeTwitterId, _, _>(
        c,
        "async/smol/snow/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_async_snow_smol::<SnowflakeTwitterId, _, _>(
        c,
        "async/smol/snow/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}

fn benchmark_async_ulid(c: &mut Criterion) {
    bench_async_ulid_tokio::<ULID, _, _, _>(
        c,
        "async/tokio/ulid/lock",
        LockMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_async_ulid_tokio::<ULID, _, _, _>(
        c,
        "async/tokio/ulid/atomic",
        AtomicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_async_ulid_smol::<ULID, _, _, _>(
        c,
        "async/smol/ulid/lock",
        LockMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_async_ulid_smol::<ULID, _, _, _>(
        c,
        "async/smol/ulid/atomic",
        AtomicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}

criterion_group!(
    name = benches;
    config = Criterion::default();
    targets =
        // --- Base32 ---
        bench_base32,
        // --- Constructors ---
        bench_constructors,
        // --- Thread locals ---
        bench_thread_local,
        // --- ULID Sequential ---
        benchmark_ulid,
        // --- ULID Async ---
        benchmark_async_ulid,
        // --- Snowflake Sequential ---
        benchmark_snow,
        // --- Snowflake Async ---
        benchmark_async_snow,
);
criterion_main!(benches);
