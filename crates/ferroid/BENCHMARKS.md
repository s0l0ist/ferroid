# Benchmarks

All numbers are Criterion medians from an Apple M1 Pro 14" (10 cores, 32 GB).
Async benches use `try_next_id_async` on multi-threaded Tokio/Smol runtimes and
measure **single-ID latency**.

```sh
cargo criterion --all-features
```

### Base32 Encoding / Decoding

Throughput is reported over the **binary ID size** (`SIZE`).

#### Snowflake (64-bit)

| Operation              | Time per call | Throughput (input) |
| ---------------------- | ------------- | ------------------ |
| `encode().as_string()` | ~17.2 ns      | ~444 MiB/s         |
| `encode().as_str()`    | ~3.31 ns      | ~2.25 GiB/s        |
| `encode_to_buf()`      | ~3.30 ns      | ~2.26 GiB/s        |
| `decode()`             | ~3.57 ns      | ~3.39 GiB/s        |

#### ULID (128-bit)

| Operation              | Time per call | Throughput (input) |
| ---------------------- | ------------- | ------------------ |
| `encode().as_string()` | ~23.3 ns      | ~656 MiB/s         |
| `encode().as_str()`    | ~5.46 ns      | ~2.73 GiB/s        |
| `encode_to_buf()`      | ~5.00 ns      | ~2.98 GiB/s        |
| `decode()`             | ~7.66 ns      | ~3.16 GiB/s        |

`encode().as_str()` and `encode_to_buf()` are allocation-free and effectively
equivalent on the hot path; `as_string()` allocates and is slower.

### Constructors

| Function                        | Time per call | Throughput     |
| ------------------------------- | ------------- | -------------- |
| `SnowflakeTwitterId::from(...)` | ~0.31 ns      | ~3.2B IDs/sec  |
| `ULID::from(...)`               | ~0.31 ns      | ~3.2B IDs/sec  |
| `ULID::from_timestamp(...)`     | ~20.9 ns      | ~47.8M IDs/sec |
| `ULID::from_datetime(...)`      | ~23.2 ns      | ~43.1M IDs/sec |
| `ULID::now()`                   | ~42.6 ns      | ~23.5M IDs/sec |

### Thread-Local ULID

| Helper                  | Time per ID | Throughput     |
| ----------------------- | ----------- | -------------- |
| `Ulid::new_ulid()`      | ~22.4 ns    | ~44.6M IDs/sec |
| `Ulid::new_mono_ulid()` | ~4.09 ns    | ~244M IDs/sec  |

### Synchronous Generators

| Generator                  | Time per ID | Throughput     |
| -------------------------- | ----------- | -------------- |
| `BasicSnowflakeGenerator`  | ~7.02 ns    | ~142M IDs/sec  |
| `LockSnowflakeGenerator`   | ~22.9 ns    | ~43.7M IDs/sec |
| `AtomicSnowflakeGenerator` | ~8.69 ns    | ~115M IDs/sec  |
| `BasicUlidGenerator`       | ~21.7 ns    | ~46.0M IDs/sec |
| `BasicMonoUlidGenerator`   | ~3.45 ns    | ~289M IDs/sec  |
| `LockMonoUlidGenerator`    | ~8.50 ns    | ~118M IDs/sec  |
| `AtomicMonoUlidGenerator`  | ~5.27 ns    | ~190M IDs/sec  |

### Async Generators (Tokio)

| Generator                  | Time per ID | Throughput     |
| -------------------------- | ----------- | -------------- |
| `LockSnowflakeGenerator`   | ~27.0 ns    | ~37.0M IDs/sec |
| `AtomicSnowflakeGenerator` | ~18.7 ns    | ~53.6M IDs/sec |
| `LockMonoUlidGenerator`    | ~8.54 ns    | ~117M IDs/sec  |
| `AtomicMonoUlidGenerator`  | ~7.06 ns    | ~142M IDs/sec  |

### Async Generators (Smol)

| Generator                  | Time per ID | Throughput     |
| -------------------------- | ----------- | -------------- |
| `LockSnowflakeGenerator`   | ~26.2 ns    | ~38.2M IDs/sec |
| `AtomicSnowflakeGenerator` | ~19.7 ns    | ~50.8M IDs/sec |
| `LockMonoUlidGenerator`    | ~8.70 ns    | ~115M IDs/sec  |
| `AtomicMonoUlidGenerator`  | ~6.95 ns    | ~144M IDs/sec  |
