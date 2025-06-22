# ferroid

[`ferroid`](https://github.com/s0l0ist/ferroid) is a Rust crate for generating
and parsing **Snowflake** and **ULID** identifiers.

## Features

- 📌 Bit-level compatibility with major Snowflake and ULID formats
- 🧩 Pluggable clocks and RNGs via `TimeSource` and `RandSource`
- 🧵 Lock-free, lock-based, and single-threaded generators
- 📐 Custom layouts via `define_snowflake_id!` and `define_ulid!` macros
- 🔢 Crockford base32 support with `base32` feature flag

[![Crates.io][crates-badge]][crates-url] [![MIT licensed][mit-badge]][mit-url]
[![Apache 2.0 licensed][apache-badge]][apache-url] [![CI][ci-badge]][ci-url]

[crates-badge]: https://img.shields.io/crates/v/ferroid.svg
[crates-url]: https://crates.io/crates/ferroid
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: https://github.com/s0l0ist/ferroid/blob/main/LICENSE-MIT
[apache-badge]: https://img.shields.io/badge/license-Apache%202.0-blue.svg
[apache-url]: https://github.com/s0l0ist/ferroid/blob/main/LICENSE-APACHE
[ci-badge]: https://github.com/s0l0ist/ferroid/actions/workflows/ci.yml/badge.svg?branch=main
[ci-url]: https://github.com/s0l0ist/ferroid/actions/workflows/ci.yml

## 📦 Supported Layouts

### Snowflake

| Platform  | Timestamp Bits | Machine ID Bits | Sequence Bits | Epoch                   |
| --------- | -------------- | --------------- | ------------- | ----------------------- |
| Twitter   | 41             | 10              | 12            | 2010-11-04 01:42:54.657 |
| Discord   | 42             | 10              | 12            | 2015-01-01 00:00:00.000 |
| Instagram | 41             | 13              | 10            | 2011-01-01 00:00:00.000 |
| Mastodon  | 48             | 0               | 16            | 1970-01-01 00:00:00.000 |

### Ulid

| Platform | Timestamp Bits | Random Bits | Epoch                   |
| -------- | -------------- | ----------- | ----------------------- |
| ULID     | 48             | 80          | 1970-01-01 00:00:00.000 |

ULIDs offer high-entropy, time-sortable IDs without coordination, but are not
strictly monotonic.

## 🔧 Generator Comparison

| Generator                  | Thread-Safe | Lock-Free | Throughput | Use Case                                    |
| -------------------------- | ----------- | --------- | ---------- | ------------------------------------------- |
| `BasicSnowflakeGenerator`  | ❌          | ❌        | Highest    | Sharded / single-threaded                   |
| `LockSnowflakeGenerator`   | ✅          | ❌        | Medium     | Fair multithreaded access                   |
| `AtomicSnowflakeGenerator` | ✅          | ✅        | High       | Fast concurrent generation (less fair)      |
| `BasicUlidGenerator`       | ✅          | ⚠️        | Lower      | Scalable, zero-coordination ULID generation |

[⚠️]: Uses thread-local RNG with no global locks, but not strictly lock-free in
the atomic/CAS sense.

Snowflake IDs are always unique and strictly ordered. ULIDs are globally
sortable but only monotonic per timestamp interval.

## 🚀 Usage

### Generate an ID

#### Synchronous

Calling `next_id()` may yield `Pending` if the current sequence is exhausted. In
that case, you can spin, yield, or sleep depending on your environment:

```rust
#[cfg(feature = "snowflake")]
{
    use ferroid::{MonotonicClock, TWITTER_EPOCH, BasicSnowflakeGenerator, SnowflakeTwitterId, IdGenStatus};

    let clock = MonotonicClock::with_epoch(TWITTER_EPOCH);
    let generator = BasicSnowflakeGenerator::new(0, clock);

    let id: SnowflakeTwitterId = loop {
        match generator.next_id() {
            IdGenStatus::Ready { id } => break id,
            IdGenStatus::Pending { yield_for } => {
                println!("Exhausted; wait for: {}ms", yield_for);
                core::hint::spin_loop();
                // Use `core::hint::spin_loop()` for single-threaded or per-thread generators.
                // Use `std::thread::yield_now()` when sharing a generator across multiple threads.
                // Use `std::thread::sleep(Duration::from_millis(yield_for.to_u64().unwrap())` to sleep.
            }
        }
    };
}

#[cfg(feature = "ulid")]
{
    use ferroid::{MonotonicClock, IdGenStatus, TWITTER_EPOCH, ThreadRandom, BasicUlidGenerator, ULID};

    let clock = MonotonicClock::with_epoch(TWITTER_EPOCH);
    let rand = ThreadRandom::default();
    let generator = BasicUlidGenerator::new(clock, rand);

    let id: ULID = loop {
        match generator.next_id() {
            IdGenStatus::Ready { id } => break id,
            IdGenStatus::Pending { yield_for } => {
                println!("Exhausted; wait for: {}ms", yield_for);
                core::hint::spin_loop();
                // Use `core::hint::spin_loop()` for single-threaded or per-thread generators.
                // Use `std::thread::yield_now()` when sharing a generator across multiple threads.
                // Use `std::thread::sleep(Duration::from_millis(yield_for.to_u64().unwrap())` to sleep.
            }
        }
    };

    println!("Generated ID: {}", id);
}
```

#### Asynchronous

If you're in an async context (e.g., using [Tokio](https://tokio.rs/) or
[Smol](https://github.com/smol-rs/smol)), you can enable one of the following
features:

- `async-tokio`
- `async-smol`

```rust
#[cfg(feature = "async-tokio")]
{
    use ferroid::{Result, MonotonicClock, MASTODON_EPOCH};

    #[tokio::main]
    async fn main() -> Result<()> {
        #[cfg(feature = "snowflake")]
        {
            use ferroid::{
                AtomicSnowflakeGenerator, SnowflakeMastodonId,
                SnowflakeGeneratorAsyncTokioExt
            };

            let clock = MonotonicClock::with_epoch(MASTODON_EPOCH);
            let generator = AtomicSnowflakeGenerator::new(0, clock);

            let id: SnowflakeMastodonId = generator.try_next_id_async().await?;
            println!("Generated ID: {}", id);
        }

        #[cfg(feature = "ulid")]
        {
            use ferroid::{ThreadRandom, UlidGeneratorAsyncTokioExt, BasicUlidGenerator, ULID};

            let clock = MonotonicClock::with_epoch(MASTODON_EPOCH);
            let rand = ThreadRandom::default();
            let generator = BasicUlidGenerator::new(clock, rand);

            let id: ULID = generator.try_next_id_async().await?;
            println!("Generated ID: {}", id);
        }
        Ok(())
    }
}

#[cfg(feature = "async-smol")]
{
    use ferroid::{Result, MonotonicClock, CUSTOM_EPOCH};

    fn main() -> Result<()> {
        smol::block_on(async {
            #[cfg(feature = "snowflake")]
            {
                use ferroid::{
                    AtomicSnowflakeGenerator, SnowflakeMastodonId,
                    SnowflakeGeneratorAsyncSmolExt
                };

                let clock = MonotonicClock::with_epoch(CUSTOM_EPOCH);
                let generator = AtomicSnowflakeGenerator::new(0, clock);

                let id: SnowflakeMastodonId = generator.try_next_id_async().await?;
                println!("Generated ID: {}", id);
            }

            #[cfg(feature = "ulid")]
            {
                use ferroid::{ThreadRandom, UlidGeneratorAsyncSmolExt, BasicUlidGenerator, ULID};

                let clock = MonotonicClock::with_epoch(CUSTOM_EPOCH);
                let rand = ThreadRandom::default();
                let generator = BasicUlidGenerator::new(clock, rand);

                let id: ULID = generator.try_next_id_async().await?;
                println!("Generated ID: {}", id);
            }

            Ok(())
        })
    }
}
```

### Custom Layouts

To define a custom layouts, use the `define_*` macros:

```rust
#[cfg(feature = "snowflake")]
{
    use ferroid::{define_snowflake_id};

    // Example: a 64-bit Twitter-like ID layout
    //
    //  Bit Index:  63           63 62            22 21             12 11             0
    //              +--------------+----------------+-----------------+---------------+
    //  Field:      | reserved (1) | timestamp (41) | machine ID (10) | sequence (12) |
    //              +--------------+----------------+-----------------+---------------+
    //              |<----------- MSB ---------- 64 bits ----------- LSB ------------>|
    define_snowflake_id!(
        MyCustomId, u64,
        reserved: 1,
        timestamp: 41,
        machine_id: 10,
        sequence: 12
    );


    // Example: a 128-bit extended ID layout
    //
    //  Bit Index:  127           88 87            40 39             20 19             0
    //              +---------------+----------------+-----------------+---------------+
    //  Field:      | reserved (40) | timestamp (48) | machine ID (20) | sequence (20) |
    //              +---------------+----------------+-----------------+---------------+
    //              |<----- HI 64 bits ----->|<-------------- LO 64 bits ------------->|
    //              |<--- MSB ------ LSB --->|<----- MSB ----- 64 bits ----- LSB ----->|
    define_snowflake_id!(
        MyCustomLongId, u128,
        reserved: 40,
        timestamp: 48,
        machine_id: 20,
        sequence: 20
    );
}

#[cfg(feature = "ulid")]
{
    use ferroid::define_ulid;

    // Example: a 128-bit ULID using the Ulid layout
    //
    // - 0 bits reserved
    // - 48 bits timestamp
    // - 80 bits random
    //
    //  Bit Index:  127            80 79           0
    //              +----------------+-------------+
    //  Field:      | timestamp (48) | random (80) |
    //              +----------------+-------------+
    //              |<-- MSB -- 128 bits -- LSB -->|
    define_ulid!(
        MyULID, u128,
        reserved: 0,
        timestamp: 48,
        random: 80
    );
}
```

> ⚠️ Note: All four sections (`reserved`, `timestamp`, `machine_id`, and
> `sequence`) must be specified in the snowflake macro, even if a section uses 0
> bits. `reserved` bits are always stored as **zero** and can be used for future
> expansion. Similarly, the ulid macro requries (`reserved`, `timestamp`, and
> `random`) fields.

### Behavior

Snowflake:

- If the clock **advances**: reset sequence to 0 → `IdGenStatus::Ready`
- If the clock is **unchanged**: increment sequence → `IdGenStatus::Ready`
- If the clock **goes backward**: return `IdGenStatus::Pending`
- If the sequence increment **overflows**: return `IdGenStatus::Pending`

Ulid:

This implementation respects monotonicity within the same millisecond for a
single generator by using the increment method.

- If the clock **advances**: generate new random → `IdGenStatus::Ready`
- If the clock is **unchanged**: increment random → `IdGenStatus::Ready`
- If the clock **goes backward**: return `IdGenStatus::Pending`
- If the random increment **overflows**: return `IdGenStatus::Pending`

### Serialize as padded string

Use `.to_padded_string()` or `.encode()` for sortable string representations:

```rust
#[cfg(feature = "snowflake")]
{
    use ferroid::{Snowflake, SnowflakeTwitterId};

    let id = SnowflakeTwitterId::from(123456, 1, 42);
    println!("default: {id}");
    // > default: 517811998762

    println!("padded: {}", id.to_padded_string());
    // > padded: 00000000517811998762

    #[cfg(feature = "base32")]
    {
        use ferroid::Base32Ext;

        let encoded = id.encode();
        println!("base32: {encoded}");
        // > base32: 00000Y4G0082M

        let decoded = SnowflakeTwitterId::decode(&encoded).expect("decode should succeed");
        assert_eq!(id, decoded);
    }
}

#[cfg(feature = "ulid")]
{
    use ferroid::{Ulid, ULID};

    let id = ULID::from(123456, 42);
    println!("default: {id}");
    // > default: 149249145986343659392525664298

    println!("padded: {}", id.to_padded_string());
    // > padded: 000000000149249145986343659392525664298
    #[cfg(feature = "base32")]
    {
        use ferroid::Base32Ext;

        let encoded = id.encode();
        println!("base32: {encoded}");
        // > base32: 000000F2800000000000000058

        let decoded = ULID::decode(&encoded).expect("decode should succeed");
        assert_eq!(id, decoded);
    }
}
```

## 📈 Benchmarks

Snowflake ID generation is theoretically capped by:

```text
max IDs/sec = 2^sequence_bits × 1000ms
```

For example, Twitter-style IDs (12 sequence bits) allow:

```text
4096 IDs/ms × 1000 ms/sec = ~4M IDs/sec
```

To benchmark this, we generate IDs in **chunks of 4096**, which aligns with the
sequence limit per millisecond in Snowflake layouts. For ULIDs, we use the same
chunk size for consistency, but this number does not represent a hard throughput
cap - ULID generation is probabilistic: monotonicity within the same millisecond
increments the random bit value. Chunking here primarily serves to keep the
benchmark code consistent.

Async benchmarks are tricky because a single generator's performance is affected
by task scheduling, which is not predictable and whose scheduler typically has a
resolution of 1 millisecond. By the time a task is scheduled to execute (i.e.,
generate an ID), a millisecond may have already passed, potentially resetting
any sequence counter or monotonic increment - thus, never truly testing the hot
path. To mitigate this, async tests measure maximum throughput: each task
generates a batch of IDs and may await on any of them. This approach offsets
idle time on one generator with active work on another, yielding more
representative throughput numbers.

### Snowflake:

- **Sync**: Benchmarks the hot path without yielding to the clock.
- **Async**: Also uses 4096-ID batches, but may yield (sequence exhaustion/CAS
  failure) or await due to task scheduling, reducing throughput.

### ULID:

- **Sync & Async**: Uses the same 4096-ID batches. Due to random number
  generation, monotonic increments may overflow randomly, reflecting real-world
  behavior.

Tests were ran on an M1 Macbook Pro 14", 32GB, 10 cores (8 performance, 2
efficiency).

#### Synchronous Generators

| Generator                | Time per ID | Throughput    |
| ------------------------ | ----------- | ------------- |
| BasicSnowflakeGenerator  | **~2.8 ns** | ~353M IDs/sec |
| LockSnowflakeGenerator   | **~8.9 ns** | ~111M IDs/sec |
| AtomicSnowflakeGenerator | **~3.1 ns** | ~320M IDs/sec |
| BasicUlidGenerator       | **~3.4 ns** | ~288M IDs/sec |
| LockUlidGenerator        | **~9.2 ns** | ~109M IDs/sec |

#### Async (Tokio Runtime) - Peak throughput

| Generator                | Generators | Time per ID  | Throughput     |
| ------------------------ | ---------- | ------------ | -------------- |
| LockSnowflakeGenerator   | 1024       | **~1.66 ns** | ~604M IDs/sec  |
| AtomicSnowflakeGenerator | 1024       | **~0.91 ns** | ~1.09B IDs/sec |
| LockUlidGenerator        | 1024       | **~1.78 ns** | ~562M IDs/sec  |

#### Async (Smol Runtime) - Peak throughput

| Generator                | Generators | Time per ID  | Throughput    |
| ------------------------ | ---------- | ------------ | ------------- |
| LockSnowflakeGenerator   | 1024       | **~1.93 ns** | ~517M IDs/sec |
| AtomicSnowflakeGenerator | 512\*      | **~1.03 ns** | ~973M IDs/sec |
| LockUlidGenerator        | 512\*      | **~1.83 ns** | ~545M IDs/sec |

\* = A higher number of generators resulted in lower throughput

To run all benchmarks:

```sh
cargo criterion --all-features
```

## 🧪 Testing

Run all tests with:

```sh
cargo test --all-features
```

## 📄 License

Licensed under either of:

- [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
  ([LICENSE-APACHE](LICENSE-APACHE))
- [MIT License](https://opensource.org/licenses/MIT)
  ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
