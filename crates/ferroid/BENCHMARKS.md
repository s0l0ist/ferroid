# Benchmarks

All numbers are Criterion medians from an Apple M1 Pro 14" (10 cores, 32 GB).

```sh
cargo criterion --features all,cache-padded,parking-lot
```

### Base32 Encoding / Decoding

Throughput is reported over the **binary ID size** (`SIZE`).

#### Snowflake (64-bit)

| Operation              | Time per call | Throughput (input) |
| ---------------------- | ------------- | ------------------ |
| `encode().as_string()` | ~17.3 ns      | ~441 MiB/s         |
| `encode().as_str()`    | ~3.33 ns      | ~2.24 GiB/s        |
| `encode_to_buf()`      | ~2.30 ns      | ~3.24 GiB/s        |
| `decode()`             | ~2.24 ns      | ~5.41 GiB/s        |

#### ULID (128-bit)

| Operation              | Time per call | Throughput (input) |
| ---------------------- | ------------- | ------------------ |
| `encode().as_string()` | ~23.2 ns      | ~655 MiB/s         |
| `encode().as_str()`    | ~5.45 ns      | ~2.73 GiB/s        |
| `encode_to_buf()`      | ~5.00 ns      | ~2.98 GiB/s        |
| `decode()`             | ~7.55 ns      | ~3.20 GiB/s        |

### Constructors

| Function                                  | Time per call | Throughput     |
| ----------------------------------------- | ------------- | -------------- |
| `SnowflakeTwitterId::from_componens(...)` | ~0.31 ns      | ~3.21B IDs/sec |
| `ULID::from_componens(...)`               | ~0.31 ns      | ~3.22B IDs/sec |
| `ULID::from_timestamp(...)`               | ~20.8 ns      | ~48.0M IDs/sec |
| `ULID::from_datetime(...)`                | ~23.0 ns      | ~43.4M IDs/sec |
| `ULID::now()`                             | ~42.2 ns      | ~23.7M IDs/sec |

### Thread-Local ULID

| Helper                      | Time per ID | Throughput     |
| --------------------------- | ----------- | -------------- |
| `Ulid::new_ulid()`          | ~21.3 ns    | ~47.0M IDs/sec |
| `Ulid::new_ulid_mono()`     | ~3.74 ns    | ~267M IDs/sec  |
| `Ulid::from_timestamp(...)` | ~20.9 ns    | ~47.8M IDs/sec |
| `Ulid::from_datetime(...)`  | ~23.2 ns    | ~43.1M IDs/sec |

### Synchronous Generators

| Generator                  | Time per ID | Throughput     |
| -------------------------- | ----------- | -------------- |
| `BasicSnowflakeGenerator`  | ~0.94 ns    | ~1.07B IDs/sec |
| `LockSnowflakeGenerator`   | ~8.20 ns    | ~122M IDs/sec  |
| `AtomicSnowflakeGenerator` | ~1.56 ns    | ~640M IDs/sec  |
| `BasicUlidGenerator`       | ~20.1 ns    | ~47.8M IDs/sec |
| `BasicMonoUlidGenerator`   | ~3.43 ns    | ~291M IDs/sec  |
| `LockMonoUlidGenerator`    | ~8.29 ns    | ~120M IDs/sec  |
| `AtomicMonoUlidGenerator`  | ~5.65 ns    | ~177M IDs/sec  |

### Async Generators (Tokio)

| Generator                  | Time per ID | Throughput    |
| -------------------------- | ----------- | ------------- |
| `LockSnowflakeGenerator`   | ~8.21 ns    | ~122M IDs/sec |
| `AtomicSnowflakeGenerator` | ~2.83 ns    | ~353M IDs/sec |
| `LockMonoUlidGenerator`    | ~8.51 ns    | ~118M IDs/sec |
| `AtomicMonoUlidGenerator`  | ~5.98 ns    | ~167M IDs/sec |

### Async Generators (Smol)

| Generator                  | Time per ID | Throughput    |
| -------------------------- | ----------- | ------------- |
| `LockSnowflakeGenerator`   | ~8.20 ns    | ~122M IDs/sec |
| `AtomicSnowflakeGenerator` | ~2.83 ns    | ~354M IDs/sec |
| `LockMonoUlidGenerator`    | ~8.50 ns    | ~118M IDs/sec |
| `AtomicMonoUlidGenerator`  | ~6.01 ns    | ~166M IDs/sec |
