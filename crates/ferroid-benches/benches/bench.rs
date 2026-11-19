//! Benchmarks for ID generation and Base32 conversion.
//!
//! Compare generator backends, direct/cached clocks, and thread-local APIs on
//! one calling thread. Shared generators are measured without contention.
//! Base32 covers reusable buffers, allocated strings, and valid decoding.
//!
//! Run with `cargo criterion -p ferroid-benches --bench bench`.
use std::hint::black_box;

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ferroid::{
    base32::Base32Ext,
    generator::{
        AtomicMonoUlidGenerator, AtomicSnowflakeGenerator, BasicMonoUlidGenerator,
        BasicSnowflakeGenerator, BasicUlidGenerator, LockMonoUlidGenerator, LockSnowflakeGenerator,
        SnowflakeGenerator, UlidGenerator,
        thread_local::{cached, direct},
    },
    id::{SnowflakeTwitterId, ULID},
    rand::ThreadRandom,
    time::{CachedClock, DirectClock, TWITTER_EPOCH},
};

ferroid::define_snowflake_id!(
    /// A single-worker layout with room for 4,194,304 IDs per millisecond.
    ///
    /// Clock reads and timestamp rollover remain part of the benchmark.
    BenchSnowflake, u64,
    reserved: 1, timestamp: 41, machine_id: 0, sequence: 22
);

fn bench_snow_base32(c: &mut Criterion) {
    let max = SnowflakeTwitterId::from_raw(u64::MAX).into_valid().to_raw();
    let ids = [0, max, max / 2, max / 3].map(SnowflakeTwitterId::from_raw);
    let encoded = ids.map(|id| id.encode());
    let mut group = c.benchmark_group("snow/base32");
    group.throughput(Throughput::Bytes(8));
    group.bench_function("encode_to_buf", |b| {
        let mut output = SnowflakeTwitterId::base32_array();
        let mut index = 0;
        b.iter(move || {
            let id = black_box(ids[index]);
            index = (index + 1) % ids.len();
            black_box(id.encode_to_buf(&mut output));
        });
    });

    group.bench_function("encode_as_string", |b| {
        let mut index = 0;
        b.iter(move || {
            let id = black_box(ids[index]);
            index = (index + 1) % ids.len();
            black_box(id.encode().as_string());
        });
    });

    group.throughput(Throughput::Bytes(encoded[0].as_bytes().len() as u64));
    group.bench_function("decode", |b| {
        let inputs = &encoded;
        let mut index = 0;
        b.iter(move || {
            let input = black_box(&inputs[index]);
            index = (index + 1) % inputs.len();
            black_box(SnowflakeTwitterId::decode(input).unwrap());
        });
    });

    group.finish();
}

fn bench_ulid_base32(c: &mut Criterion) {
    let max = u128::MAX;
    let ids = [0, max, max / 2, max / 3].map(ULID::from_raw);
    let encoded = ids.map(|id| id.encode());
    let mut group = c.benchmark_group("ulid/base32");
    group.throughput(Throughput::Bytes(16));
    group.bench_function("encode_to_buf", |b| {
        let mut output = ULID::base32_array();
        let mut index = 0;
        b.iter(move || {
            let id = black_box(ids[index]);
            index = (index + 1) % ids.len();
            black_box(id.encode_to_buf(&mut output));
        });
    });

    group.bench_function("encode_as_string", |b| {
        let mut index = 0;
        b.iter(move || {
            let id = black_box(ids[index]);
            index = (index + 1) % ids.len();
            black_box(id.encode().as_string());
        });
    });

    group.throughput(Throughput::Bytes(encoded[0].as_bytes().len() as u64));
    group.bench_function("decode", |b| {
        let inputs = &encoded;
        let mut index = 0;
        b.iter(move || {
            let input = black_box(&inputs[index]);
            index = (index + 1) % inputs.len();
            black_box(ULID::decode(input).unwrap());
        });
    });

    group.finish();
}

fn bench_snow_generators(c: &mut Criterion) {
    let mut group = c.benchmark_group("snow/generator/cached");
    group.throughput(Throughput::Elements(1));
    group.bench_function("basic", |b| {
        let generator = BasicSnowflakeGenerator::<BenchSnowflake, _>::new(
            0,
            CachedClock::<1>::with_epoch(TWITTER_EPOCH),
        );
        b.iter(|| {
            let id = generator
                .try_next_id(|_| panic!("ready benchmark reached backoff"))
                .unwrap();
            black_box(id);
        });
    });

    group.bench_function("lock", |b| {
        let generator = LockSnowflakeGenerator::<BenchSnowflake, _>::new(
            0,
            CachedClock::<1>::with_epoch(TWITTER_EPOCH),
        );
        b.iter(|| {
            let id = generator
                .try_next_id(|_| panic!("ready benchmark reached backoff"))
                .unwrap();
            black_box(id);
        });
    });

    group.bench_function("atomic", |b| {
        let generator = AtomicSnowflakeGenerator::<BenchSnowflake, _>::new(
            0,
            CachedClock::<1>::with_epoch(TWITTER_EPOCH),
        );
        b.iter(|| {
            let id = generator
                .try_next_id(|_| panic!("ready benchmark reached backoff"))
                .unwrap();
            black_box(id);
        });
    });

    group.finish();
}

fn bench_ulid_generators(c: &mut Criterion) {
    let mut group = c.benchmark_group("ulid/generator/cached");
    group.throughput(Throughput::Elements(1));
    group.bench_function("basic", |b| {
        let generator = BasicUlidGenerator::<ULID, _, _>::new(CachedClock::default(), ThreadRandom);
        b.iter(|| {
            let id = generator
                .try_next_id(|_| panic!("ready benchmark reached backoff"))
                .unwrap();
            black_box(id);
        });
    });

    group.bench_function("basic_mono", |b| {
        let generator =
            BasicMonoUlidGenerator::<ULID, _, _>::new(CachedClock::default(), ThreadRandom);
        b.iter(|| {
            let id = generator
                .try_next_id(|_| panic!("ready benchmark reached backoff"))
                .unwrap();
            black_box(id);
        });
    });

    group.bench_function("lock_mono", |b| {
        let generator =
            LockMonoUlidGenerator::<ULID, _, _>::new(CachedClock::default(), ThreadRandom);
        b.iter(|| {
            let id = generator
                .try_next_id(|_| panic!("ready benchmark reached backoff"))
                .unwrap();
            black_box(id);
        });
    });

    group.bench_function("atomic_mono", |b| {
        let generator =
            AtomicMonoUlidGenerator::<ULID, _, _>::new(CachedClock::default(), ThreadRandom);
        b.iter(|| {
            let id = generator
                .try_next_id(|_| panic!("ready benchmark reached backoff"))
                .unwrap();
            black_box(id);
        });
    });

    group.finish();
}

fn bench_ulid_generation(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate/ulid");
    group.throughput(Throughput::Elements(1));
    group.bench_function("now", |b| {
        b.iter(|| black_box(ULID::now()));
    });
    group.bench_function("basic", |b| {
        let generator = BasicUlidGenerator::<ULID, _, _>::new(DirectClock::default(), ThreadRandom);
        b.iter(|| black_box(generator.next_id(|_| std::thread::yield_now())));
    });
    group.bench_function("basic_mono", |b| {
        let generator =
            BasicMonoUlidGenerator::<ULID, _, _>::new(DirectClock::default(), ThreadRandom);
        b.iter(|| black_box(generator.next_id(|_| std::thread::yield_now())));
    });
    group.bench_function("lock_mono", |b| {
        let generator =
            LockMonoUlidGenerator::<ULID, _, _>::new(DirectClock::default(), ThreadRandom);
        b.iter(|| {
            black_box(generator.try_next_id(|_| std::thread::yield_now()).unwrap());
        });
    });
    group.bench_function("atomic_mono", |b| {
        let generator =
            AtomicMonoUlidGenerator::<ULID, _, _>::new(DirectClock::default(), ThreadRandom);
        b.iter(|| black_box(generator.next_id(|_| std::thread::yield_now())));
    });
    group.finish();
}

fn bench_thread_local(c: &mut Criterion) {
    let mut group = c.benchmark_group("thread_local/ulid");
    group.throughput(Throughput::Elements(1));
    group.bench_function("new_ulid/direct", |b| {
        black_box(direct::Ulid::new_ulid());
        b.iter(|| black_box(direct::Ulid::new_ulid()));
    });

    group.bench_function("new_ulid_mono/direct", |b| {
        black_box(direct::Ulid::new_ulid_mono(|_| std::thread::yield_now()));
        b.iter(|| black_box(direct::Ulid::new_ulid_mono(|_| std::thread::yield_now())));
    });

    group.bench_function("new_ulid/cached", |b| {
        black_box(cached::Ulid::new_ulid());
        b.iter(|| black_box(cached::Ulid::new_ulid()));
    });
    group.bench_function("new_ulid_mono/cached", |b| {
        black_box(cached::Ulid::new_ulid_mono(|_| std::thread::yield_now()));
        b.iter(|| black_box(cached::Ulid::new_ulid_mono(|_| std::thread::yield_now())));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_snow_base32,
    bench_ulid_base32,
    bench_ulid_generation,
    bench_snow_generators,
    bench_ulid_generators,
    bench_thread_local,
);
criterion_main!(benches);
