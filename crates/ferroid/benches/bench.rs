use core::{fmt, hint::black_box};
use criterion::{
    Criterion, Throughput, async_executor::SmolExecutor, criterion_group, criterion_main,
};
use ferroid::{
    base32::{Base32SnowExt, Base32UlidExt},
    define_snowflake_id, define_ulid,
    futures::{SmolSleep, SnowflakeGeneratorAsyncExt, TokioSleep, UlidGeneratorAsyncExt},
    generator::{
        AtomicMonoUlidGenerator, AtomicSnowflakeGenerator, BasicMonoUlidGenerator,
        BasicSnowflakeGenerator, BasicUlidGenerator, IdGenStatus, LockMonoUlidGenerator,
        LockSnowflakeGenerator, SnowflakeGenerator, UlidGenerator, thread_local::Ulid,
    },
    id::{BeBytes, SnowflakeId, UlidId},
    rand::{RandSource, ThreadRandom},
    time::TimeSource,
};
use portable_atomic::{AtomicU64, Ordering};
use tokio::runtime::Builder;

define_snowflake_id!(
    /// A snowflake that contains enough sequence bits to test the hot path
    BenchSnowflake, u64,
    reserved: 1,
    timestamp: 0,
    machine_id: 0,
    sequence: 63
);

define_ulid!(
    /// A ulid that contains enough sequence bits to test the hot path
    BenchUlid, u128,
    reserved: 1,
    timestamp: 0,
    random: 127
);

/// A clock that simulates the atomic load from `MonotonicClock`, but which
/// never increments, therefore guaranteeing the generator(s) will never yield.
#[derive(Default)]
struct BenchClock {
    time: AtomicU64,
}
impl TimeSource<u64> for BenchClock {
    fn current_millis(&self) -> u64 {
        self.time.load(Ordering::Relaxed)
    }
}
impl TimeSource<u128> for BenchClock {
    fn current_millis(&self) -> u128 {
        u128::from(self.time.load(Ordering::Relaxed))
    }
}

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
            black_box(BenchUlid::now());
        });
    });
    group.bench_function("from", |b| {
        b.iter(|| {
            black_box(BenchUlid::from(41, 42));
        });
    });
    group.bench_function("from_timestamp", |b| {
        b.iter(|| {
            black_box(BenchUlid::from_timestamp(42));
        });
    });
    group.bench_function("from_datetime", |b| {
        let now = std::time::SystemTime::now();
        b.iter(|| {
            black_box(BenchUlid::from_datetime(now));
        });
    });
    group.finish();
}
fn bench_snow_constructors(c: &mut Criterion, group_name: &str) {
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("from", |b| {
        b.iter(|| {
            black_box(BenchSnowflake::from(40, 41, 42));
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
    T: TimeSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("next_id", |b| {
        let clock = clock_fn();
        let g = generator_fn(0, clock);
        b.iter(|| {
            loop {
                match g.next_id() {
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
        b.iter(|| {
            loop {
                match g.next_id() {
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
    T: TimeSource<ID::Ty> + Send,
{
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("try_next_id_async", |b| {
        let rt = Builder::new_multi_thread().enable_all().build().unwrap();
        let clock = clock_fn();
        let g = generator_fn(0, clock);
        b.to_async(&rt).iter(async || {
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
        b.to_async(SmolExecutor).iter(async || {
            let id = g.try_next_id_async::<SmolSleep>().await.unwrap();
            black_box(id);
        });
    });
    group.finish();
}
/// Benchmarks the latency of generating a single BenchUlid in an async context
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
        b.to_async(&rt).iter(async || {
            let id = g.try_next_id_async::<TokioSleep>().await.unwrap();
            black_box(id);
        });
    });
    group.finish();
}
/// Benchmarks the latency of generating a single BenchUlid in an async context
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
    // unsafe { std::env::set_var("SMOL_THREADS", num_cpus::get().to_string()) };
    let mut group = c.benchmark_group(group_name);
    group.throughput(Throughput::Elements(1));
    group.bench_function("try_next_id_async", |b| {
        let clock = clock_fn();
        let rand = rand_fn();
        let g = generator_fn(clock, rand);
        b.to_async(SmolExecutor).iter(async || {
            let id = g.try_next_id_async::<SmolSleep>().await.unwrap();
            black_box(id);
        });
    });
    group.finish();
}

// Base32 encode/decode
fn bench_base32(c: &mut Criterion) {
    bench_snow_base32::<BenchSnowflake>(c, "base32/snow");
    bench_ulid_base32::<BenchUlid>(c, "base32/ulid");
}
fn bench_constructors(c: &mut Criterion) {
    bench_snow_constructors(c, "BenchSnowflake");
    bench_ulid_constructors(c, "BenchUlid");
}
fn bench_thread_local(c: &mut Criterion) {
    bench_thread_local_ulid(c, "thread_local/Ulid");
}
fn benchmark_ulid(c: &mut Criterion) {
    bench_generator_ulid::<BenchUlid, _, _, _>(
        c,
        "basic/ulid",
        BasicUlidGenerator::new,
        BenchClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<BenchUlid, _, _, _>(
        c,
        "basic_mono/ulid",
        BasicMonoUlidGenerator::new,
        BenchClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<BenchUlid, _, _, _>(
        c,
        "lock_mono/ulid",
        LockMonoUlidGenerator::new,
        BenchClock::default,
        ThreadRandom::default,
    );
    bench_generator_ulid::<BenchUlid, _, _, _>(
        c,
        "atomic_mono/ulid",
        AtomicMonoUlidGenerator::new,
        BenchClock::default,
        ThreadRandom::default,
    );
}
fn benchmark_snow(c: &mut Criterion) {
    bench_generator_snow::<BenchSnowflake, _, _>(
        c,
        "basic/snow",
        BasicSnowflakeGenerator::new,
        BenchClock::default,
    );
    bench_generator_snow::<BenchSnowflake, _, _>(
        c,
        "lock/snow",
        LockSnowflakeGenerator::new,
        BenchClock::default,
    );
    bench_generator_snow::<BenchSnowflake, _, _>(
        c,
        "atomic/snow",
        AtomicSnowflakeGenerator::new,
        BenchClock::default,
    );
}
fn benchmark_async_ulid(c: &mut Criterion) {
    bench_async_ulid_tokio::<BenchUlid, _, _, _>(
        c,
        "tokio/lock_mono/ulid",
        LockMonoUlidGenerator::new,
        BenchClock::default,
        ThreadRandom::default,
    );
    bench_async_ulid_smol::<BenchUlid, _, _, _>(
        c,
        "smol/lock_mono/ulid",
        LockMonoUlidGenerator::new,
        BenchClock::default,
        ThreadRandom::default,
    );
    bench_async_ulid_tokio::<BenchUlid, _, _, _>(
        c,
        "tokio/atomic_mono/ulid",
        AtomicMonoUlidGenerator::new,
        BenchClock::default,
        ThreadRandom::default,
    );
    bench_async_ulid_smol::<BenchUlid, _, _, _>(
        c,
        "smol/atomic_mono/ulid",
        AtomicMonoUlidGenerator::new,
        BenchClock::default,
        ThreadRandom::default,
    );
}
fn benchmark_async_snow(c: &mut Criterion) {
    bench_async_snow_tokio::<BenchSnowflake, _, _>(
        c,
        "tokio/lock/snow",
        LockSnowflakeGenerator::new,
        BenchClock::default,
    );
    bench_async_snow_smol::<BenchSnowflake, _, _>(
        c,
        "smol/lock/snow",
        LockSnowflakeGenerator::new,
        BenchClock::default,
    );
    bench_async_snow_tokio::<BenchSnowflake, _, _>(
        c,
        "tokio/atomic/snow",
        AtomicSnowflakeGenerator::new,
        BenchClock::default,
    );
    bench_async_snow_smol::<BenchSnowflake, _, _>(
        c,
        "smol/atomic/snow",
        AtomicSnowflakeGenerator::new,
        BenchClock::default,
    );
}
criterion_group!(
    name = benches;
    config = Criterion::default()
    // .sample_size(100)
    // .warm_up_time(std::time::Duration::from_micros(500))
    // .measurement_time(std::time::Duration::from_millis(1))
    ;
    targets =
        // --- Base32 Encoding/Decoding ---
        bench_base32,
        // --- ID Constructors ---
        bench_constructors,
        // --- Thread-Local Generation ---
        bench_thread_local,
        // --- BenchUlid Synchronous Generation ---
        benchmark_ulid,
        // --- Snowflake Synchronous Generation ---
        benchmark_snow,
        // // --- BenchUlid Async Generation ---
        benchmark_async_ulid,
        // --- Snowflake Async Generation ---
        benchmark_async_snow,
);
criterion_main!(benches);
