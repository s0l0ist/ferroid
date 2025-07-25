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

## 🔧 Generator Comparison

| Snowflake Generator        | Monotonic | Thread-Safe | Lock-Free | Throughput | Use Case                                |
| -------------------------- | --------- | ----------- | --------- | ---------- | --------------------------------------- |
| `BasicSnowflakeGenerator`  | ✅        | ❌          | ❌        | Highest    | Single-threaded or generator per thread |
| `LockSnowflakeGenerator`   | ✅        | ✅          | ❌        | Medium     | Fair multithreaded access               |
| `AtomicSnowflakeGenerator` | ✅        | ✅          | ✅        | High       | Fast concurrent generation (less fair)  |

| Ulid Generator       | Monotonic | Thread-Safe | Lock-Free | Throughput | Use Case                                |
| -------------------- | --------- | ----------- | --------- | ---------- | --------------------------------------- |
| `BasicUlidGenerator` | ✅        | ❌          | ❌        | Highest    | Single-threaded or generator per thread |
| `LockUlidGenerator`  | ✅        | ✅          | ❌        | Medium     | Fair multithreaded access               |

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
                core::hint::spin_loop(); // Blocking spin: burns CPU, but yields the lowest latency.
                // std::thread::yield_now(); // Optional: yields to OS, still busy-waits.
                // std::thread::sleep(Duration::from_millis(yield_for.to_u64().unwrap())); // Lowest CPU use, but imprecise and may oversleep.
                //
                // For non-blocking ID generation, use the async API (see below).
            }
        }
    };
}

#[cfg(feature = "ulid")]
{
    use ferroid::{MonotonicClock, IdGenStatus, UNIX_EPOCH, ThreadRandom, BasicUlidGenerator, ULID};

    let clock = MonotonicClock::with_epoch(UNIX_EPOCH);
    let rand = ThreadRandom::default();
    let generator = BasicUlidGenerator::new(clock, rand);

    let id: ULID = loop {
        match generator.next_id() {
            IdGenStatus::Ready { id } => break id,
            IdGenStatus::Pending { yield_for } => {
                println!("Exhausted; wait for: {}ms", yield_for);
                core::hint::spin_loop(); // Blocking spin: burns CPU, but yields the lowest latency.
                // std::thread::yield_now(); // Optional: yields to OS, still busy-waits.
                // std::thread::sleep(Duration::from_millis(yield_for.to_u64().unwrap())); // Lowest CPU use, but imprecise and may oversleep.
                //
                // For non-blocking ID generation, use the async API (see below).
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
    use ferroid::{Result, MonotonicClock, MASTODON_EPOCH, UNIX_EPOCH};

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

            let clock = MonotonicClock::with_epoch(UNIX_EPOCH);
            let rand = ThreadRandom::default();
            let generator = BasicUlidGenerator::new(clock, rand);

            let id: ULID = generator.try_next_id_async().await?;
            println!("Generated ID: {}", id);
        }
        Ok(())
    }
    main().expect("failed to run")
}

#[cfg(feature = "async-smol")]
{
    use ferroid::{Result, MonotonicClock};

    fn main() -> Result<()> {
        smol::block_on(async {
            #[cfg(feature = "snowflake")]
            {
                use ferroid::{
                    AtomicSnowflakeGenerator, SnowflakeMastodonId,
                    SnowflakeGeneratorAsyncSmolExt, CUSTOM_EPOCH
                };

                let clock = MonotonicClock::with_epoch(CUSTOM_EPOCH);
                let generator = AtomicSnowflakeGenerator::new(0, clock);

                let id: SnowflakeMastodonId = generator.try_next_id_async().await?;
                println!("Generated ID: {}", id);
            }

            #[cfg(feature = "ulid")]
            {
                use ferroid::{ThreadRandom, UlidGeneratorAsyncSmolExt, BasicUlidGenerator, ULID, UNIX_EPOCH};

                let clock = MonotonicClock::with_epoch(UNIX_EPOCH);
                let rand = ThreadRandom::default();
                let generator = BasicUlidGenerator::new(clock, rand);

                let id: ULID = generator.try_next_id_async().await?;
                println!("Generated ID: {}", id);
            }

            Ok(())
        })
    }
    main().expect("failed to run")
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

#### Snowflake

- If the clock **advances**: reset sequence to 0 → `IdGenStatus::Ready`
- If the clock is **unchanged**: increment sequence → `IdGenStatus::Ready`
- If the clock **goes backward**: return `IdGenStatus::Pending`
- If the sequence increment **overflows**: return `IdGenStatus::Pending`

#### Ulid

This implementation respects monotonicity within the same millisecond in a
single generator by incrementing the random portion of the ID and guarding
against overflow.

- If the clock **advances**: generate new random → `IdGenStatus::Ready`
- If the clock is **unchanged**: increment random → `IdGenStatus::Ready`
- If the clock **goes backward**: return `IdGenStatus::Pending`
- If the random increment **overflows**: return `IdGenStatus::Pending`

### Probability of ID Collisions

When generating time-sortable IDs that use random bits, it's important to
estimate the probability of collisions (i.e., two IDs being the same within the
same millisecond), given your ID layout and system throughput.

#### Monotonic IDs with Multiple ULID Generators

If you have $g$ generators (e.g., distributed nodes), and each generator
produces $k$ **sequential** (monotonic) IDs per millisecond by incrementing from
a random starting point, the probability that any two generators produce
overlapping IDs in the same millisecond is approximately:

$$P_\text{collision} \approx \frac{g(g-1)(2k-1)}{2 \cdot 2^r}$$

Where:

- $g$ = number of generators
- $k$ = number of monotonic IDs per generator per millisecond
- $r$ = number of random bits per ID
- $P_\text{collision}$ = probability of at least one collision

> Note:
> The formula above uses the approximate (birthday bound) model, which assumes that:
>
> - $k \ll 2^r$ and $g \ll 2^r$
> - Each generator's range of $k$ IDs starts at a uniformly random position within the $r$-bit space

#### Estimating Time Until a Collision Occurs

While collisions only happen within a single millisecond, we often want to know how long it takes before **any** collision happens, given continuous generation over time.

The expected time in milliseconds to reach a 50% chance of collision is:

$T_{\text{50\%}} \approx \frac{\ln 2}{P_\text{collision}} = \frac{0.6931 \cdot 2 \cdot 2^r}{g(g - 1)(2k - 1)}$

This is derived from the cumulative probability formula:

$P_\text{collision}(T) = 1 - (1 - P_\text{collision})^T$

Solving for $T$ when $P_\text{collision}(T) = 0.5$:

$(1 - P_\text{collision})^T = 0.5$

$\Rightarrow T \approx \frac{\ln(0.5)}{\ln(1 - P_\text{collision})}$

Using the approximation $\ln(1 - x) \approx -x$ for small $x$, this simplifies to:

$\Rightarrow T \approx \frac{\ln 2}{P_\text{collision}}$

The $\ln 2$ term arises because $\ln(0.5) = -\ln 2$. After $T_\text{50\%}$ milliseconds, there's a 50% chance that at least one collision has occurred.

| Generators ($g$) | IDs per generator per ms ($k$) | $P_\text{collision}$                                                                                    | Estimated Time to 50% Collision ($T_{\text{50\%}}$)         |
| ---------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| 1                | 1                              | $0$ (single generator; no collision possible)                                                           | ∞ (no collision possible)                                   |
| 1                | 65,536                         | $0$ (single generator; no collision possible)                                                           | ∞ (no collision possible)                                   |
| 2                | 1                              | $\displaystyle \frac{2 \times 1 \times 1}{2 \cdot 2^{80}} \approx 8.27 \times 10^{-25}$                 | $\approx 8.38 \times 10^{23} \text{ ms}$                    |
| 2                | 65,536                         | $\displaystyle \frac{2 \times 1 \times 131{,}071}{2 \cdot 2^{80}} \approx 1.08 \times 10^{-19}$         | $\approx 6.41 \times 10^{18} \text{ ms}$                    |
| 1,000            | 1                              | $\displaystyle \frac{1{,}000 \times 999 \times 1}{2 \cdot 2^{80}} \approx 4.13 \times 10^{-19}$         | $\approx 1.68 \times 10^{18} \text{ ms}$                    |
| 1,000            | 65,536                         | $\displaystyle \frac{1{,}000 \times 999 \times 131{,}071}{2 \cdot 2^{80}} \approx 5.42 \times 10^{-14}$ | $\approx 1.28 \times 10^{13} \text{ ms} \approx 406\ years$ |

### Serialize as padded string

Use `.encode()` or `.encode_to_buf()` for sortable string representations:

```rust
#[cfg(all(feature = "base32", feature = "snowflake"))]
{
    use ferroid::{Base32SnowExt, Snowflake, SnowflakeTwitterId};

    let id = SnowflakeTwitterId::from(123456, 1, 42);
    assert_eq!(format!("default: {id}"), "default: 517811998762");

    let encoded = id.encode();
    assert_eq!(format!("base32: {encoded}"), "base32: 00000F280041A");

    let decoded = SnowflakeTwitterId::decode(&encoded).expect("decode should succeed");
    assert_eq!(id, decoded);
}

#[cfg(all(feature = "base32", feature = "ulid"))]
{
    use ferroid::{Base32UlidExt, Ulid, ULID};

    let id = ULID::from(123456, 42);
    assert_eq!(format!("default: {id}"), "default: 149249145986343659392525664298");

    let encoded = id.encode();
    assert_eq!(format!("base32: {encoded}"), "base32: 0000003RJ0000000000000001A");

    let decoded = ULID::decode(&encoded).expect("decode should succeed");
    assert_eq!(decoded.timestamp(), 123456);
    assert_eq!(decoded.random(), 42);
    assert_eq!(id, decoded);

    let decoded = ULID::decode("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
    assert_eq!(decoded.timestamp(), 1469922850259);
    assert_eq!(decoded.random(), 1012768647078601740696923);
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
  behavior. In general, it is rare for ULIDs to overflow.

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
| LockSnowflakeGenerator   | 1024       | **~1.46 ns** | ~687M IDs/sec  |
| AtomicSnowflakeGenerator | 1024       | **~0.86 ns** | ~1.17B IDs/sec |
| LockUlidGenerator        | 1024       | **~1.57 ns** | ~635M IDs/sec  |

#### Async (Smol Runtime) - Peak throughput

| Generator                | Generators | Time per ID  | Throughput     |
| ------------------------ | ---------- | ------------ | -------------- |
| LockSnowflakeGenerator   | 1024       | **~1.40 ns** | ~710M IDs/sec  |
| AtomicSnowflakeGenerator | 1024       | **~0.62 ns** | ~1.61B IDs/sec |
| LockUlidGenerator        | 1024       | **~1.32 ns** | ~756M IDs/sec  |

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
