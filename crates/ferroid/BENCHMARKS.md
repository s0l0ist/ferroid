# Benchmarks

All numbers are Criterion medians from an Apple M1 Pro 14" (10 cores, 32 GB).

```sh
cargo criterion --all-features
```

### Base32 Encoding / Decoding

Throughput is reported over the **binary ID size** (`SIZE`).

#### Snowflake (64-bit)

| Operation              | Time per call | Throughput (input) |
| ---------------------- | ------------- | ------------------ |
| `encode().as_string()` | ~17.7 ns      | ~432 MiB/s         |
| `encode().as_str()`    | ~3.37 ns      | ~2.21 GiB/s        |
| `encode_to_buf()`      | ~3.31 ns      | ~2.25 GiB/s        |
| `decode()`             | ~3.65 ns      | ~3.32 GiB/s        |

#### ULID (128-bit)

| Operation              | Time per call | Throughput (input) |
| ---------------------- | ------------- | ------------------ |
| `encode().as_string()` | ~24.0 ns      | ~635 MiB/s         |
| `encode().as_str()`    | ~5.64 ns      | ~2.64 GiB/s        |
| `encode_to_buf()`      | ~5.01 ns      | ~2.97 GiB/s        |
| `decode()`             | ~7.70 ns      | ~3.15 GiB/s        |

### Constructors

| Function                        | Time per call | Throughput     |
| ------------------------------- | ------------- | -------------- |
| `SnowflakeTwitterId::from(...)` | ~0.31 ns      | ~3.21B IDs/sec |
| `ULID::from(...)`               | ~0.31 ns      | ~3.19B IDs/sec |
| `ULID::from_timestamp(...)`     | ~21.4 ns      | ~46.8M IDs/sec |
| `ULID::from_datetime(...)`      | ~23.1 ns      | ~43.4M IDs/sec |
| `ULID::now()`                   | ~42.2 ns      | ~23.7M IDs/sec |

### Thread-Local ULID

| Helper                      | Time per ID | Throughput     |
| --------------------------- | ----------- | -------------- |
| `Ulid::new_ulid()`          | ~21.6 ns    | ~46.4M IDs/sec |
| `Ulid::new_mono_ulid()`     | ~4.06 ns    | ~246M IDs/sec  |
| `Ulid::from_timestamp(...)` | ~21.1 ns    | ~47.5M IDs/sec |
| `Ulid::from_datetime(...)`  | ~23.7 ns    | ~42.1M IDs/sec |

### Synchronous Generators

| Generator                  | Time per ID | Throughput     |
| -------------------------- | ----------- | -------------- |
| `BasicSnowflakeGenerator`  | ~0.94 ns    | ~1.07B IDs/sec |
| `LockSnowflakeGenerator`   | ~8.23 ns    | ~121M IDs/sec  |
| `AtomicSnowflakeGenerator` | ~1.56 ns    | ~641M IDs/sec  |
| `BasicUlidGenerator`       | ~21.3 ns    | ~47.0M IDs/sec |
| `BasicMonoUlidGenerator`   | ~3.44 ns    | ~291M IDs/sec  |
| `LockMonoUlidGenerator`    | ~8.33 ns    | ~120M IDs/sec  |
| `AtomicMonoUlidGenerator`  | ~5.32 ns    | ~188M IDs/sec  |

### Async Generators (Tokio)

| Generator                  | Time per ID | Throughput    |
| -------------------------- | ----------- | ------------- |
| `LockSnowflakeGenerator`   | ~8.26 ns    | ~121M IDs/sec |
| `AtomicSnowflakeGenerator` | ~3.75 ns    | ~267M IDs/sec |
| `LockMonoUlidGenerator`    | ~8.37 ns    | ~119M IDs/sec |
| `AtomicMonoUlidGenerator`  | ~7.03 ns    | ~142M IDs/sec |

### Async Generators (Smol)

| Generator                  | Time per ID | Throughput    |
| -------------------------- | ----------- | ------------- |
| `LockSnowflakeGenerator`   | ~8.22 ns    | ~122M IDs/sec |
| `AtomicSnowflakeGenerator` | ~3.17 ns    | ~316M IDs/sec |
| `LockMonoUlidGenerator`    | ~8.37 ns    | ~119M IDs/sec |
| `AtomicMonoUlidGenerator`  | ~6.90 ns    | ~145M IDs/sec |
