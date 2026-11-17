//! Benchmarks for the public `ferroid` API.
//!
//! Run with `cargo criterion -p ferroid-benches --bench bench`.
use core::num::NonZeroU64;
use std::{future::pending, hint::black_box};

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ferroid::{
    base32::Base32Ext,
    futures::{SnowflakeGeneratorAsyncExt, UlidGeneratorAsyncExt},
    generator::{
        AtomicMonoUlidGenerator, AtomicSnowflakeGenerator, BasicMonoUlidGenerator,
        BasicSnowflakeGenerator, BasicUlidGenerator, LockMonoUlidGenerator, LockSnowflakeGenerator,
        SnowflakeGenerator, UlidGenerator,
        thread_local::{cached, direct},
    },
    id::{SnowflakeTwitterId, ULID},
    rand::{RandSource, ThreadRandom},
    time::{CachedClock, DirectClock, TWITTER_EPOCH, TimeSource},
};
use futures::FutureExt;

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

    group.bench_function("encode_as_str", |b| {
        let mut index = 0;
        b.iter(move || {
            let id = black_box(ids[index]);
            index = (index + 1) % ids.len();
            let encoded = id.encode();
            black_box(encoded.as_str());
        });
    });

    group.bench_function("encode_to_buf_as_str", |b| {
        let mut output = SnowflakeTwitterId::base32_array();
        let mut index = 0;
        b.iter(move || {
            let id = black_box(ids[index]);
            index = (index + 1) % ids.len();
            black_box(id.encode_to_buf(&mut output).as_str());
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

    // Put the invalid character last to exercise the complete cold scan.
    let mut invalid = SnowflakeTwitterId::base32_array();
    invalid.copy_from_slice(encoded[3].as_bytes());
    *invalid.last_mut().unwrap() = b'!';
    assert!(SnowflakeTwitterId::decode(invalid).is_err());
    group.bench_function("decode_invalid_ascii", |b| {
        b.iter(|| black_box(SnowflakeTwitterId::decode(black_box(&invalid))));
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

    group.bench_function("encode_as_str", |b| {
        let mut index = 0;
        b.iter(move || {
            let id = black_box(ids[index]);
            index = (index + 1) % ids.len();
            let encoded = id.encode();
            black_box(encoded.as_str());
        });
    });

    group.bench_function("encode_to_buf_as_str", |b| {
        let mut output = ULID::base32_array();
        let mut index = 0;
        b.iter(move || {
            let id = black_box(ids[index]);
            index = (index + 1) % ids.len();
            black_box(id.encode_to_buf(&mut output).as_str());
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

    // Put the invalid character last to exercise the complete cold scan.
    let mut invalid = ULID::base32_array();
    invalid.copy_from_slice(encoded[3].as_bytes());
    *invalid.last_mut().unwrap() = b'!';
    assert!(ULID::decode(invalid).is_err());
    group.bench_function("decode_invalid_ascii", |b| {
        b.iter(|| black_box(ULID::decode(black_box(&invalid))));
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

fn bench_pending_generators(c: &mut Criterion) {
    struct FixedClock(u64);
    impl TimeSource<u64> for FixedClock {
        const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(8).unwrap();

        fn current_millis(&self) -> u64 {
            black_box(self.0)
        }
    }
    impl TimeSource<u128> for FixedClock {
        const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(8).unwrap();

        fn current_millis(&self) -> u128 {
            u128::from(black_box(self.0))
        }
    }
    struct ZeroRand;
    impl RandSource<u128> for ZeroRand {
        fn rand(&self) -> u128 {
            0
        }
    }

    // Fixed clocks keep both pending paths deterministic without sleeping.
    let mut group = c.benchmark_group("snow/generator/pending");
    for (case, now, sequence) in [
        ("exhaustion", 42, BenchSnowflake::max_sequence()),
        ("rollback", 39, 0),
    ] {
        let basic = BasicSnowflakeGenerator::<BenchSnowflake, _>::from_components(
            42,
            0,
            sequence,
            FixedClock(now),
        );
        group.bench_function(format!("basic/{case}"), |b| {
            b.iter(|| black_box(basic.poll_id()));
        });
        let atomic = AtomicSnowflakeGenerator::<BenchSnowflake, _>::from_components(
            42,
            0,
            sequence,
            FixedClock(now),
        );
        group.bench_function(format!("atomic/{case}"), |b| {
            b.iter(|| black_box(atomic.poll_id()));
        });
        let lock = LockSnowflakeGenerator::<BenchSnowflake, _>::from_components(
            42,
            0,
            sequence,
            FixedClock(now),
        );
        group.bench_function(format!("lock/{case}"), |b| {
            b.iter(|| black_box(lock.try_poll_id().unwrap()));
        });
    }
    group.finish();

    let mut group = c.benchmark_group("ulid/generator/pending");
    for (case, now, random) in [("exhaustion", 42, ULID::max_random()), ("rollback", 39, 0)] {
        let basic = BasicMonoUlidGenerator::<ULID, _, _>::from_components(
            42,
            random,
            FixedClock(now),
            ZeroRand,
        );
        group.bench_function(format!("basic_mono/{case}"), |b| {
            b.iter(|| black_box(basic.poll_id()));
        });
        let atomic = AtomicMonoUlidGenerator::<ULID, _, _>::from_components(
            42,
            random,
            FixedClock(now),
            ZeroRand,
        );
        group.bench_function(format!("atomic_mono/{case}"), |b| {
            b.iter(|| black_box(atomic.poll_id()));
        });
        let lock = LockMonoUlidGenerator::<ULID, _, _>::from_components(
            42,
            random,
            FixedClock(now),
            ZeroRand,
        );
        group.bench_function(format!("lock_mono/{case}"), |b| {
            b.iter(|| black_box(lock.try_poll_id().unwrap()));
        });
    }
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

fn bench_generate_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("generate_encode/ulid");
    group.throughput(Throughput::Elements(1));
    group.bench_function("now", |b| {
        let mut output = ULID::base32_array();
        b.iter(move || {
            black_box(ULID::now().encode_to_buf(&mut output));
        });
    });
    group.bench_function("basic_mono", |b| {
        let generator =
            BasicMonoUlidGenerator::<ULID, _, _>::new(DirectClock::default(), ThreadRandom);
        let mut output = ULID::base32_array();
        b.iter(move || {
            let id = generator.next_id(|_| std::thread::yield_now());
            black_box(id.encode_to_buf(&mut output));
        });
    });
    group.bench_function("cached_mono", |b| {
        let generator =
            BasicMonoUlidGenerator::<ULID, _, _>::new(CachedClock::default(), ThreadRandom);
        let mut output = ULID::base32_array();
        b.iter(move || {
            let id = generator.next_id(|_| std::thread::yield_now());
            black_box(id.encode_to_buf(&mut output));
        });
    });
    group.finish();
}

fn bench_async_ready(c: &mut Criterion) {
    // Poll each future once to measure the ready path without an executor.
    let mut group = c.benchmark_group("async/ready");
    group.throughput(Throughput::Elements(1));
    group.bench_function("snow", |b| {
        let generator = BasicSnowflakeGenerator::<BenchSnowflake, _>::new(
            0,
            CachedClock::<1>::with_epoch(TWITTER_EPOCH),
        );
        b.iter(|| {
            let id = generator
                .next_id_async(|_| pending::<()>())
                .now_or_never()
                .expect("ready benchmark reached backoff");
            black_box(id);
        });
    });

    group.bench_function("ulid", |b| {
        let generator =
            BasicMonoUlidGenerator::<ULID, _, _>::new(CachedClock::default(), ThreadRandom);
        b.iter(|| {
            let id = generator
                .next_id_async(|_| pending::<()>())
                .now_or_never()
                .expect("ready benchmark reached backoff");
            black_box(id);
        });
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

fn bench_providers(c: &mut Criterion) {
    let mut group = c.benchmark_group("clock/read");
    group.throughput(Throughput::Elements(1));
    group.bench_function("direct", |b| {
        let clock = DirectClock::default();
        b.iter(|| black_box(<DirectClock as TimeSource<u64>>::current_millis(&clock)));
    });

    group.bench_function("cached", |b| {
        let clock = CachedClock::default();
        b.iter(|| black_box(<CachedClock as TimeSource<u64>>::current_millis(&clock)));
    });

    group.finish();

    let mut group = c.benchmark_group("rng");
    group.throughput(Throughput::Elements(1));
    group.bench_function("thread/u128", |b| {
        let rng = ThreadRandom;
        b.iter(|| black_box(<ThreadRandom as RandSource<u128>>::rand(&rng)));
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_snow_base32,
    bench_ulid_base32,
    bench_ulid_generation,
    bench_generate_encode,
    bench_snow_generators,
    bench_ulid_generators,
    bench_pending_generators,
    bench_async_ready,
    bench_thread_local,
    bench_providers,
);
criterion_main!(benches);
