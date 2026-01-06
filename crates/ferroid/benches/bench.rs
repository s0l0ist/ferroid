use core::{fmt, hint::black_box};

use criterion::{
    Criterion, Throughput, async_executor::SmolExecutor, criterion_group, criterion_main,
};
use ferroid::{
    base32::{Base32SnowExt, Base32UlidExt},
    define_snowflake_id,
    futures::{SmolSleep, SnowflakeGeneratorAsyncExt, TokioSleep, UlidGeneratorAsyncExt},
    generator::{
        AtomicMonoUlidGenerator, AtomicSnowflakeGenerator, BasicMonoUlidGenerator,
        BasicSnowflakeGenerator, BasicUlidGenerator, LockMonoUlidGenerator, LockSnowflakeGenerator,
        SnowflakeGenerator, UlidGenerator, thread_local::Ulid,
    },
    id::{BeBytes, SnowflakeId, SnowflakeTwitterId, ULID, UlidId},
    rand::{RandSource, ThreadRandom},
    time::{MonotonicClock, TimeSource},
};
use tokio::runtime::Builder;

define_snowflake_id!(
    /// A snowflake that contains enough sequence bits to test the hot path
    BenchSnowflake, u64,
    reserved: 1,
    timestamp: 0,
    machine_id: 0,
    sequence: 63
);

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
    group.bench_function("encode_as_string", |b| {
        b.iter(|| {
            black_box(id.encode().as_string());
        });
    });
    group.bench_function("encode_as_str", |b| {
        b.iter(|| {
            black_box(id.encode().as_str());
        });
    });
    group.bench_function("encode_to_buf", |b| {
        let mut buf = ID::base32_array();
        b.iter(|| {
            let b = id.encode_to_buf(&mut buf);
            black_box(b);
        });
    });
    group.throughput(Throughput::Bytes(<ID::Ty as BeBytes>::BASE32_SIZE as u64));
    group.bench_function("decode", |b| {
        let encoded = id.encode();
        b.iter(|| {
            black_box(ID::decode(&encoded).unwrap());
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
    group.bench_function("encode_as_string", |b| {
        b.iter(|| {
            black_box(id.encode().as_string());
        });
    });
    group.bench_function("encode_as_str", |b| {
        b.iter(|| {
            black_box(id.encode().as_str());
        });
    });
    group.bench_function("encode_to_buf", |b| {
        let mut buf = ID::base32_array();
        b.iter(|| {
            let b = id.encode_to_buf(&mut buf);
            black_box(b);
        });
    });
    group.throughput(Throughput::Bytes(<ID::Ty as BeBytes>::BASE32_SIZE as u64));
    group.bench_function("decode", |b| {
        let encoded = id.encode();
        b.iter(|| {
            black_box(ID::decode(&encoded).unwrap());
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
    group.bench_function("from_components", |b| {
        b.iter(|| {
            black_box(ULID::from_components(41, 42));
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
    group.bench_function("from_components", |b| {
        b.iter(|| {
            black_box(SnowflakeTwitterId::from_components(40, 41, 42));
        });
    });
    group.finish();
}
fn bench_ulid_thread_local(c: &mut Criterion, group_name: &str) {
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("new_ulid", |b| {
        b.iter(|| {
            black_box(Ulid::new_ulid());
        });
    });
    let backoff = |_| std::thread::yield_now();
    group.bench_function("new_ulid_mono", |b| {
        b.iter(|| {
            black_box(Ulid::new_ulid_mono(backoff));
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
    T: TimeSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("next_id", |b| {
        let clock = clock_fn();
        let g = generator_fn(0, clock);
        let backoff = |_| core::hint::spin_loop();
        b.iter(|| {
            let id = g.try_next_id(backoff).unwrap();
            black_box(id);
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
    G: UlidGenerator<ID, T, R>,
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("next_id", |b| {
        let clock = clock_fn();
        let rand = rand_fn();
        let g = generator_fn(clock, rand);
        let backoff = |_| core::hint::spin_loop();
        b.iter(|| {
            let id = g.try_next_id(backoff).unwrap();
            black_box(id);
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
    T: TimeSource<ID::Ty> + Send,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("try_next_id_async", |b| {
        let rt = Builder::new_multi_thread().enable_all().build().unwrap();
        let clock = clock_fn();
        let g = generator_fn(0, clock);
        b.to_async(&rt).iter(|| async {
            let id = g.try_next_id_async::<TokioSleep>().await.unwrap();
            black_box(id);
        });
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
    T: TimeSource<ID::Ty> + Send,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("try_next_id_async", |b| {
        let clock = clock_fn();
        let g = generator_fn(0, clock);
        b.to_async(SmolExecutor).iter(|| async {
            let id = g.try_next_id_async::<SmolSleep>().await.unwrap();
            black_box(id);
        });
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
    group.bench_function("try_next_id_async", |b| {
        let rt = Builder::new_multi_thread().enable_all().build().unwrap();
        let clock = clock_fn();
        let rand = rand_fn();
        let g = generator_fn(clock, rand);
        b.to_async(&rt).iter(|| async {
            let id = g.try_next_id_async::<TokioSleep>().await.unwrap();
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
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("try_next_id_async", |b| {
        let clock = clock_fn();
        let rand = rand_fn();
        let g = generator_fn(clock, rand);
        b.to_async(SmolExecutor).iter(|| async {
            let id = g.try_next_id_async::<SmolSleep>().await.unwrap();
            black_box(id);
        });
    });
    group.finish();
}

fn bench_constructors_snow(c: &mut Criterion) {
    bench_snow_constructors(c, "snow");
}

fn bench_constructors_ulid(c: &mut Criterion) {
    bench_ulid_constructors(c, "ulid");
}

fn bench_base32_snow(c: &mut Criterion) {
    bench_snow_base32::<SnowflakeTwitterId>(c, "snow/base32");
}
fn bench_base32_ulid(c: &mut Criterion) {
    bench_ulid_base32::<ULID>(c, "ulid/base32");
}

fn bench_thread_local_ulid(c: &mut Criterion) {
    bench_ulid_thread_local(c, "thread_local/ulid");
}
fn benchmark_ulid(c: &mut Criterion) {
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "ulid/basic",
        BasicUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "ulid/basic_mono",
        BasicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "ulid/lock_mono",
        LockMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<ULID, _, _, _>(
        c,
        "ulid/atomic_mono",
        AtomicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}
fn benchmark_snow(c: &mut Criterion) {
    // These use `BenchSnowflake` to avoid pending
    bench_generator_snow::<BenchSnowflake, _, _>(
        c,
        "snow/basic",
        BasicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_generator_snow::<BenchSnowflake, _, _>(
        c,
        "snow/lock",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_generator_snow::<BenchSnowflake, _, _>(
        c,
        "snow/atomic",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}
fn benchmark_async_ulid(c: &mut Criterion) {
    bench_async_ulid_tokio::<ULID, _, _, _>(
        c,
        "ulid/lock_mono/tokio",
        LockMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_async_ulid_smol::<ULID, _, _, _>(
        c,
        "ulid/lock_mono/smol",
        LockMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_async_ulid_tokio::<ULID, _, _, _>(
        c,
        "ulid/atomic_mono/tokio",
        AtomicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
    bench_async_ulid_smol::<ULID, _, _, _>(
        c,
        "ulid/atomic_mono/smol",
        AtomicMonoUlidGenerator::new,
        MonotonicClock::default,
        ThreadRandom::default,
    );
}
fn benchmark_async_snow(c: &mut Criterion) {
    bench_async_snow_tokio::<BenchSnowflake, _, _>(
        c,
        "snow/lock/tokio",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_async_snow_smol::<BenchSnowflake, _, _>(
        c,
        "snow/lock/smol",
        LockSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_async_snow_tokio::<BenchSnowflake, _, _>(
        c,
        "snow/atomic/tokio",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
    bench_async_snow_smol::<BenchSnowflake, _, _>(
        c,
        "snow/atomic/smol",
        AtomicSnowflakeGenerator::new,
        MonotonicClock::default,
    );
}
criterion_group!(
    name = benches;
    config = Criterion::default()
    // .warm_up_time(std::time::Duration::from_millis(500))
    // .measurement_time(std::time::Duration::from_millis(500))
    ;
    targets =
        // --- ID Constructors ---
        bench_constructors_ulid,
         // --- Base32 Encoding/Decoding ---
        bench_base32_ulid,
        // --- Thread-Local Generation ---
        bench_thread_local_ulid,
        // --- ULID Synchronous Generation ---
        benchmark_ulid,
        // --- ULID Async Generation ---
        benchmark_async_ulid,

        // --- ID Constructors ---
        bench_constructors_snow,
         // --- Base32 Encoding/Decoding ---
        bench_base32_snow,
         // --- Snowflake Synchronous Generation ---
        benchmark_snow,
        // --- Snowflake Async Generation ---
        benchmark_async_snow,
);
criterion_main!(benches);
