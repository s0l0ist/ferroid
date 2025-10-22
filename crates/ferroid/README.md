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
| `AtomicSnowflakeGenerator` | ✅        | ✅          | ✅        | High       | Fast concurrent generation              |

| Ulid Generator            | Monotonic | Thread-Safe | Lock-Free | Throughput | Use Case                                |
| ------------------------- | --------- | ----------- | --------- | ---------- | --------------------------------------- |
| `BasicUlidGenerator`      | ❌        | ✅          | ❌        | Slow       | Thread-safe, always random, but slow    |
| `BasicMonoUlidGenerator`  | ✅        | ❌          | ❌        | Highest    | Single-threaded or generator per thread |
| `LockMonoUlidGenerator`   | ✅        | ✅          | ❌        | Medium     | Fair multithreaded access               |
| `AtomicMonoUlidGenerator` | ✅        | ✅          | ✅        | High       | Fast concurrent generation              |

## 🚀 Usage

### Thread Locals

The simplest way to generate a ULID is via `Ulid`, which provides a thread-local
generator that can produce both non-monotonic and monotonic ULIDs:

```rust
use ferroid::{id::ULID, generator::thread_local::Ulid};

// A ULID (slower, always random within the same millisecond)
let id: ULID = Ulid::new_ulid();

// A monotonic ULID (faster, increments within the same millisecond)
let id: ULID = Ulid::new_mono_ulid();
```

Thread-local generators are not currently available for `SnowflakeId`-style IDs
because they rely on a valid `machine_id` to avoid collisions. Mapping unique
`machine_id`s across threads requires coordination beyond what `thread_local!`
alone can guarantee.

## Serde

Users must explicitly choose a serialization strategy using `#[serde(with =
"...")]`:

There are two serialization strategies:

- `as_native_snow`/`as_native_ulid`: Serialize as native integer types
  (u64/u128)
- `as_base32_snow`/`as_base32_ulid`: Serialize as Crockford base32 encoded
  strings

Both strategies validate during deserialization and return errors for invalid
IDs. This prevents overflow scenarios where the underlying integer value exceeds
the valid range for the ID type. For example, `SnowflakeTwitterId` reserves 1
bit, making `u64::MAX` invalid. This validation behavior is consistent with
`Base32Error::DecodeOverflow` used in the base32 decoding path (see next
section).

```rust
use ferroid::{id::SnowflakeTwitterId, serde::{as_base32_snow, as_native_snow}};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Event {
    #[serde(with = "as_native_snow")]
    id_snow_int: SnowflakeTwitterId, // Serializes as an int: 123456789
    #[serde(with = "as_base32_snow")]
    id_snow_base32: SnowflakeTwitterId, // Serializes as a base32 string: "000000000001A"
}
```

### Crockford Base32

Enable the `base32` feature to support Crockford Base32 encoding and decoding of
IDs. This is useful when you need fixed-width, URL-safe, and lexicographically
sortable strings (e.g. for databases, logs, or URLs).

With `base32` enabled, each ID type automatically implements `fmt::Display`,
which internally uses `.encode()`. IDs also implement `TryFrom<&str>` and
`FromStr`, both of which decode via `.decode()`.

For explicit, allocation-free formatting, use `.encode()` to get a lightweight
formatter. This avoids committing to a specific string type and lets the
consumer control how and when to render the result. The formatter uses a
stack-allocated buffer and avoids heap allocation by default. To enable
`.to_string()` and other owned string functionality, enable the `alloc` feature.

```rust
use core::str::FromStr;
use ferroid::{
    id::{SnowflakeId, SnowflakeTwitterId, ULID, UlidId},
    base32::{Base32SnowExt, Base32SnowFormatter, Base32UlidExt, Base32UlidFormatter},
};


let id = SnowflakeTwitterId::from(123_456, 0, 42);
assert_eq!(format!("{id}"), "00000F280001A");
assert_eq!(id.encode(), "00000F280001A");
assert_eq!(SnowflakeTwitterId::decode("00000F280001A").unwrap(), id);
assert_eq!(SnowflakeTwitterId::try_from("00000F280001A").unwrap(), id);
assert_eq!(SnowflakeTwitterId::from_str("00000F280001A").unwrap(), id);

let id = ULID::from(123_456, 42);
assert_eq!(format!("{id}"), "0000003RJ0000000000000001A");
assert_eq!(id.encode(), "0000003RJ0000000000000001A");
assert_eq!(ULID::decode("0000003RJ0000000000000001A").unwrap(), id);
assert_eq!(ULID::try_from("0000003RJ0000000000000001A").unwrap(), id);
assert_eq!(ULID::from_str("0000003RJ0000000000000001A").unwrap(), id);
```

⚠️ Decoding and Overflow: ULID Spec vs. Ferroid

Base32 encodes in 5-bit chunks. That means:

- A `u32` (32 bits) maps to 7 Base32 characters (7 × 5 = 35 bits)
- A `u64` (64 bits) maps to 13 Base32 characters (13 × 5 = 65 bits)
- A `u128` (128 bits) maps to 26 Base32 characters (26 × 5 = 130 bits)

This creates an invariant: an encoded string may contain more bits than the
target type can hold.

The [ULID
specification](https://github.com/ulid/spec?tab=readme-ov-file#overflow-errors-when-parsing-base32-strings)
is strict:

> Technically, a 26-character Base32 encoded string can contain 130 bits of
> information, whereas a ULID must only contain 128 bits. Therefore, the largest
> valid ULID encoded in Base32 is 7ZZZZZZZZZZZZZZZZZZZZZZZZZ, which corresponds
> to an epoch time of 281474976710655 or 2 ^ 48 - 1.
>
> Any attempt to decode or encode a ULID larger than this should be rejected by
> all implementations, to prevent overflow bugs.

Ferroid takes a more flexible stance:

- Strings like `"ZZZZZZZZZZZZZZZZZZZZZZZZZZ"` (which technically overflow) are
  accepted and decoded without error.
- However, if any of the overflowed bits fall into reserved regions, which must
  remain zero, decoding will fail with `Base32Error::DecodeOverflow`.

This allows any 13-character Base32 string to decode into a `u64`, or any
26-character string into a `u128`, **as long as reserved layout constraints
aren't violated**. If the layout defines no reserved bits, decoding is always
considered valid.

For example:

- A `ULID` has no reserved bits, so decoding will never fail due to overflow.
- A `SnowflakeTwitterId` reserves the highest bit, so decoding must ensure that
  bit remains unset.

If reserved bits are set during decoding, Ferroid returns a
`Base32Error::DecodeOverflow { id }` containing the full (invalid) ID. You can
recover by calling `.into_valid()` to mask off reserved bits-allowing either
explicit error handling or silent correction.

### Generate an ID

#### Clocks

In `std` environments, you can use the default `MonotonicClock` implementation.
It is thread-safe, lightweight to clone, and intended to be shared across the
application. If you're using multiple generators, clone and reuse the same clock
instance.

By default, `MonotonicClock::default()` sets the offset to `UNIX_EPOCH`. You
should override this depending on the ID specification. For example, Twitter IDs
use `TWITTER_EPOCH`, which begins at **Thursday, November 4, 2010, 01:42:54.657
UTC** (millisecond zero).

```rust
use ferroid::time::{MonotonicClock, UNIX_EPOCH};

// Same as MonotonicClock::default();
let clock = MonotonicClock::with_epoch(UNIX_EPOCH);

// let generator0 = BasicSnowflakeGenerator::new(0, clock.clone());
// let generator1 = BasicSnowflakeGenerator::new(1, clock.clone());
```

#### Synchronous Generators

Calling `next_id()` may yield `Pending` if the current sequence is exhausted.
Please note that while this behavior is exposed to provide maximum flexibility,
you must be generating enough IDs **per millisecond** to draw out the `Pending`
path. You may spin, yield, or sleep depending on your environment:

```rust
use ferroid::{
    rand::ThreadRandom,
    generator::{IdGenStatus, BasicSnowflakeGenerator, BasicUlidGenerator},
    id::{SnowflakeTwitterId, ToU64, ULID},
    time::{MonotonicClock, TWITTER_EPOCH},
};

let snow_gen = BasicSnowflakeGenerator::new(0, MonotonicClock::with_epoch(TWITTER_EPOCH));
let id: SnowflakeTwitterId = loop {
    match snow_gen.next_id() {
        IdGenStatus::Ready { id } => break id,
        IdGenStatus::Pending { yield_for } => {
            // Spin: lowest latency, but generally avoid.
            core::hint::spin_loop();
            // Yield to the scheduler: lets another thread run; still may busy-wait.
            std::thread::yield_now();
            // Sleep for the suggested backoff: frees the core, but wakeup is imprecise.
            std::thread::sleep(std::time::Duration::from_millis(yield_for.to_u64()));
            // For use in runtimes such as `tokio` or `smol`, use the async API (see below).
        }
    }
};

let ulid_gen = BasicUlidGenerator::new(MonotonicClock::default(), ThreadRandom::default());
let id: ULID = loop {
    match ulid_gen.next_id() {
        IdGenStatus::Ready { id } => break id,
        IdGenStatus::Pending { yield_for } => {
            std::thread::yield_now();
        }
    }
};
```

#### Asynchronous Generators

If you're in an async context (e.g., using [Tokio](https://tokio.rs/) or
[Smol](https://github.com/smol-rs/smol)), enable one of the following features
to avoid blocking behavior:

- `aysnc-tokio`
- `async-smol`

These features extend the generator to yield cooperatively when it returns
`Pending`, causing the current task to sleep for the specified `yield_for`
duration (typically ~1ms). While this is fully non-blocking, it may oversleep
slightly due to OS or executor timing precision, potentially reducing peak
throughput.

```rust
use ferroid::{
    Error, generator::{LockMonoUlidGenerator, LockSnowflakeGenerator}, time::MASTODON_EPOCH, time::MonotonicClock, Result,
    id::{SnowflakeMastodonId, ULID}, rand::ThreadRandom, time::UNIX_EPOCH,
    futures::{SnowflakeGeneratorAsyncTokioExt, UlidGeneratorAsyncTokioExt},
};

async fn run() -> Result<(), Error> {
    let snow_gen = LockSnowflakeGenerator::new(0, MonotonicClock::with_epoch(MASTODON_EPOCH));
    let id: SnowflakeMastodonId = snow_gen.try_next_id_async().await?;
    println!("Generated ID: {}", id);

    let ulid_gen = LockMonoUlidGenerator::new(
        MonotonicClock::with_epoch(UNIX_EPOCH),
        ThreadRandom::default(),
    );
    let id: ULID = ulid_gen.try_next_id_async().await?;
    println!("Generated ID: {}", id);
    Ok(())
}

fn async_tokio_main() -> Result<(), Error> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build Tokio runtime")
        .block_on(run())
}

fn async_smol_main() -> Result<(), Error> {
    smol::block_on(run())
}

fn main() -> Result<(), Error> {
    let t1 = std::thread::spawn(async_tokio_main);
    let t2 = std::thread::spawn(async_smol_main);

    t1.join().expect("tokio thread panicked")?;
    t2.join().expect("smol thread panicked")?;
    Ok(())
}
```

### Custom Layouts

To gain more control or optimize for different performance characteristics, you
can define a custom layout.

Use the `define_*` macros below to create a new struct with your chosen name.
The resulting type behaves just like built-in types such as `SnowflakeTwitterId`
or `ULID`, with no extra setup required and full compatibility with the existing
API.

```rust
use ferroid::{define_snowflake_id, define_ulid};

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
```

⚠️ Note: When using the snowflake macro, you must specify all four sections (in
order): `reserved`, `timestamp`, `machine_id`, and `sequence`-even if a section
uses 0 bits.

The reserved bits are always set to **zero** and can be reserved for future use.

Similarly, the ulid macro requires all three fields: `reserved`, `timestamp`,
and `random`.

### Feature flags

Ferroid has many features flags to enable only what you need. You should
determine your runtime and pick at least one ID family and generator style:

- Determine your runtime: `std` (+ `alloc`), `no_std`, or `no_std` + `alloc`
- ID family: `snowflake` or `ulid` (or `thread-local` ULID generator)
- Generator: `basic`, `lock`, or `atomic`

Prefer `basic` or `atomic` generators. `lock` is a fallback for targets without
viable atomics. `cache-padded` and `parking-lot` only matter for lock-based
generators.

In `no_std`, you're currently limited to using the `basic` and `atomic`
generators provided the target platform supports the correct atomic widths for
`snowflake` (`AtomicU64`), or `ulid` (`AtomicU128`). You also must create your
own implementation of `TimeSource<T>` for the generator(s). `base32` is also
supported.

- `all`: Enables all functionality (except optimizing `cache-padded`,
  `parking-lot`).
- `std`: Required for `MonotonicClock`, the `thread-local` (`Ulid` generator),
  and all lock-based generators.
- `alloc`: Enables `ToString` and allocating String functions when `base32` is
  also enabled.
- `cache-padded`: Pads contended generators to reduce false sharing. Benchmark
  to confirm benefit.
- `parking-lot`: Use `parking_lot` mutexes for lock generators (implies `std`,
  `alloc`).
- `thread-local`: Per-thread ULID generator (implies `std`, `alloc`, `ulid`,
  `basic`).
- `snowflake`: Enable Snowflake ID generators.
- `ulid`: Enable ULID ID generators.
- `basic`: Enable basic (fast-path) generators.
- `lock`: Enable lock-based generators (implies `std`, `alloc`).
- `atomic`: Enable lock-free/atomic generators.
- `async-tokio`: Async extensions for Tokio (implies `std`, `alloc`, `futures`).
- `async-smol`: Async extensions for smol (implies `std`, `alloc`, `futures`).
- `futures`: Internal glue for the async features.
- `base32`: Crockford Base32 encode/decode support.
- `tracing`: Emit tracing spans during ID generation.
- `serde`: Enable serde on ID types.

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

> Note: The formula above uses the approximate (birthday bound) model, which
> assumes that:
>
> - $k \ll 2^r$ and $g \ll 2^r$
> - Each generator's range of $k$ IDs starts at a uniformly random position
>   within the $r$-bit space

#### Estimating Time Until a Collision Occurs

While collisions only happen within a single millisecond, we often want to know
how long it takes before **any** collision happens, given continuous generation
over time.

The expected time in milliseconds to reach a 50% chance of collision is:

$T_{\text{50\%}} \approx \frac{\ln 2}{P_\text{collision}} = \frac{0.6931 \cdot 2
\cdot 2^r}{g(g - 1)(2k - 1)}$

This is derived from the cumulative probability formula:

$P_\text{collision}(T) = 1 - (1 - P_\text{collision})^T$

Solving for $T$ when $P_\text{collision}(T) = 0.5$:

$(1 - P_\text{collision})^T = 0.5$

$\Rightarrow T \approx \frac{\ln(0.5)}{\ln(1 - P_\text{collision})}$

Using the approximation $\ln(1 - x) \approx -x$ for small $x$, this simplifies
to:

$\Rightarrow T \approx \frac{\ln 2}{P_\text{collision}}$

The $\ln 2$ term arises because $\ln(0.5) = -\ln 2$. After $T_\text{50\%}$
milliseconds, there's a 50% chance that at least one collision has occurred.

| Generators ($g$) | IDs per generator per ms ($k$) | $P_\text{collision}$                                                                                    | Estimated Time to 50% Collision ($T_{\text{50\%}}$)         |
| ---------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| 1                | 1                              | $0$ (single generator; no collision possible)                                                           | ∞ (no collision possible)                                   |
| 1                | 65,536                         | $0$ (single generator; no collision possible)                                                           | ∞ (no collision possible)                                   |
| 2                | 1                              | $\displaystyle \frac{2 \times 1 \times 1}{2 \cdot 2^{80}} \approx 8.27 \times 10^{-25}$                 | $\approx 8.38 \times 10^{23} \text{ ms}$                    |
| 2                | 65,536                         | $\displaystyle \frac{2 \times 1 \times 131{,}071}{2 \cdot 2^{80}} \approx 1.08 \times 10^{-19}$         | $\approx 6.41 \times 10^{18} \text{ ms}$                    |
| 1,000            | 1                              | $\displaystyle \frac{1{,}000 \times 999 \times 1}{2 \cdot 2^{80}} \approx 4.13 \times 10^{-19}$         | $\approx 1.68 \times 10^{18} \text{ ms}$                    |
| 1,000            | 65,536                         | $\displaystyle \frac{1{,}000 \times 999 \times 131{,}071}{2 \cdot 2^{80}} \approx 5.42 \times 10^{-14}$ | $\approx 1.28 \times 10^{13} \text{ ms} \approx 406\ years$ |

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

| Generator                  | Time per ID  | Throughput    |
| -------------------------- | ------------ | ------------- |
| `BasicSnowflakeGenerator`  | **~2.2 ns**  | ~450M IDs/sec |
| `LockSnowflakeGenerator`   | **~8.5 ns**  | ~118M IDs/sec |
| `AtomicSnowflakeGenerator` | **~3.4 ns**  | ~297M IDs/sec |
| `BasicUlidGenerator`       | **~21.7 ns** | ~46M IDs/sec  |
| `BasicMonoUlidGenerator`   | **~3.6 ns**  | ~281M IDs/sec |
| `LockMonoUlidGenerator`    | **~8.5 ns**  | ~118M IDs/sec |
| `AtomicMonoUlidGenerator`  | **~5.1 ns**  | ~194M IDs/sec |

#### Thread Local Generators

| Generator             | Time per ID  | Throughput     |
| --------------------- | ------------ | -------------- |
| `Ulid::new_ulid`      | **~23.5 ns** | ~42.6M IDs/sec |
| `Ulid::new_mono_ulid` | **~5.1 ns**  | ~195M IDs/sec  |

#### Async (Tokio Runtime) - Peak throughput

| Generator                  | Generators | Time per ID  | Throughput     |
| -------------------------- | ---------- | ------------ | -------------- |
| `LockSnowflakeGenerator`   | 1024       | **~1.18 ns** | ~849M IDs/sec  |
| `AtomicSnowflakeGenerator` | 1024       | **~0.80 ns** | ~1.25B IDs/sec |
| `LockMonoUlidGenerator`    | 1024       | **~1.19 ns** | ~838M IDs/sec  |
| `AtomicMonoUlidGenerator`  | 1024       | **~1.01 ns** | ~992M IDs/sec  |

#### Async (Smol Runtime) - Peak throughput

| Generator                  | Generators | Time per ID  | Throughput     |
| -------------------------- | ---------- | ------------ | -------------- |
| `LockSnowflakeGenerator`   | 1024       | **~1.17 ns** | ~852M IDs/sec  |
| `AtomicSnowflakeGenerator` | 1024       | **~0.76 ns** | ~1.32B IDs/sec |
| `LockMonoUlidGenerator`    | 1024       | **~1.19 ns** | ~842M IDs/sec  |
| `AtomicMonoUlidGenerator`  | 1024       | **~0.98 ns** | ~1.02B IDs/sec |

To run all benchmarks:

```sh
cargo criterion --all-features
```

## 🧪 Testing

Run all tests with:

```sh
cargo test --features all
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
