# ferroid

[`ferroid`](https://github.com/s0l0ist/ferroid) is a Rust crate for generating
and parsing **Snowflake-style unique IDs**.

It supports pre-built layouts for platforms like Twitter, Discord, Instagram,
and Mastodon. These IDs are 64-bit integers that encode timestamps,
machine/shard IDs, and sequence numbers - making them **lexicographically
sortable**, **scalable**, and ideal for **distributed systems**.

Features:

- 📌 Bit-level layout compatibility with major Snowflake formats
- 🧩 Pluggable time sources via the `TimeSource` trait
- 🧵 Lock-based and lock-free thread-safe ID generation
- 📐 Customizable layouts via the `Snowflake` trait
- 🔢 Lexicographically sortable string encoding

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

| Platform  | Timestamp Bits | Machine ID Bits | Sequence Bits | Epoch                   |
| --------- | -------------- | --------------- | ------------- | ----------------------- |
| Twitter   | 41             | 10              | 12            | 2010-11-04 01:42:54.657 |
| Discord   | 42             | 10              | 12            | 2015-01-01 00:00:00.000 |
| Instagram | 41             | 13              | 10            | 2011-01-01 00:00:00.000 |
| Mastodon  | 48             | 0               | 16            | 1970-01-01 00:00:00.000 |

## 🔧 Generator Comparison

| Generator                  | Thread-Safe | Lock-Free | Throughput | Use Case                                                                       |
| -------------------------- | ----------- | --------- | ---------- | ------------------------------------------------------------------------------ |
| `BasicSnowflakeGenerator`  | ❌          | ❌        | Highest    | Single-threaded, zero contention; ideal for sharded/core-local generators      |
| `LockSnowflakeGenerator`   | ✅          | ❌        | Medium     | Multi-threaded workloads where fair access across threads is important         |
| `AtomicSnowflakeGenerator` | ✅          | ✅        | High       | Multi-threaded workloads where fair access is sacrificed for higher throughput |

All generators produce **monotonically increasing**, **time-ordered**, and
**unique** IDs.

## 🚀 Usage

### Generate an ID

#### Synchronous

Calling `next_id()` may yield `Pending` if the current sequence is exhausted. In
that case, you can spin, yield, or sleep depending on your environment:

```rust
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

println!("Generated ID: {}", id);
```

#### Asynchronous

If you're in an async context (e.g., using [Tokio](https://tokio.rs/) or
[Smol](https://github.com/smol-rs/smol)), you can enable one of the following
features:

- `async-tokio` - for integration with the Tokio runtime
- `async-smol` - for integration with the Smol runtime

Then, import the corresponding `SnowflakeGeneratorAsyncTokioExt` or
`SnowflakeGeneratorAsyncSmolExt` trait to asynchronously request a new ID
without blocking or spinning.

Tokio Example

```rust
use ferroid::{
    AtomicSnowflakeGenerator, MASTODON_EPOCH, MonotonicClock, Result, SnowflakeGeneratorAsyncTokioExt,
    SnowflakeMastodonId, TokioSleep,
};

#[tokio::main]
async fn main() -> Result<()> {
    let clock = MonotonicClock::with_epoch(MASTODON_EPOCH);
    let generator = AtomicSnowflakeGenerator::new(0, clock);

    let id: SnowflakeMastodonId = generator.try_next_id_async().await?;
    println!("Generated ID: {}", id);

    Ok(())
}
```

Smol Example

```rust
use ferroid::{
    AtomicSnowflakeGenerator, MASTODON_EPOCH, MonotonicClock, Result, SmolSleep,
    SnowflakeGeneratorAsyncSmolExt, SnowflakeMastodonId,
};

fn main() -> Result<()> {
    smol::block_on(async {
        let clock = MonotonicClock::with_epoch(MASTODON_EPOCH);
        let generator = AtomicSnowflakeGenerator::new(0, clock);

        let id: SnowflakeMastodonId = generator.try_next_id_async().await?;
        println!("Generated ID: {}", id);

        Ok(())
    })
}
```

### Custom Layouts

To define a custom Snowflake layout, use the `define_snowflake_id` macro:

```rust
use ferroid::define_snowflake_id;

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
//  Bit Index:  127                88 87            40 39             20 19             0
//              +--------------------+----------------+-----------------+---------------+
//  Field:      | reserved (40 bits) | timestamp (48) | machine ID (20) | sequence (20) |
//              +--------------------+----------------+-----------------+---------------+
//              |<--- HI 64 bits --->|<------------------- LO 64 bits ----------------->|
//              |<- MSB ------ LSB ->|<----- MSB ---------- 64 bits --------- LSB ----->|
define_snowflake_id!(
    MyCustomLongId, u128,
    reserved: 40,
    timestamp: 48,
    machine_id: 20,
    sequence: 20
);
```

> Note: All four sections (`reserved`, `timestamp`, `machine_id`, and `sequence`) must be
> specified in the macro, even if a section uses 0 bits. `reserved` bits are always
> stored as **zero** and can be used for future expansion.

### Behavior

- If the clock **advances**: reset sequence to 0 → `IdGenStatus::Ready`
- If the clock is **unchanged**: increment sequence → `IdGenStatus::Ready`
- If the clock **goes backward**: return `IdGenStatus::Pending`
- If the sequence **overflows**: return `IdGenStatus::Pending`

### Serialize as padded string

Use `.to_padded_string()` or `.encode()` (enabled with `base32` feature) for
sortable representations:

```rust
use ferroid::{SnowflakeTwitterId, SnowflakeBase32Ext};

let id = SnowflakeTwitterId::from(123456, 1, 42);
println!("default: {id}");
// > default: 517811998762

println!("padded: {}", id.to_padded_string());
// > padded: 00000000517811998762

let encoded = id.encode();
println!("base32: {encoded}");
// > base32: 00000Y4G0082M

let decoded = SnowflakeTwitterId::decode(&encoded).expect("decode should succeed");
assert_eq!(id, decoded);
```

## 📈 Benchmarks

`ferroid` ships with Criterion benchmarks to measure ID generation performance.

Here's a snapshot of peak **single-core** throughput on a MacBook Pro 14" M1 (8
performance + 2 efficiency cores), measured under ideal conditions where the
generator never yields. These numbers reflect the upper bounds of real-clock
performance:

```bash
mono/sequential/basic/elems/4096
    time:   [11.747 µs 11.809 µs 11.885 µs]
    thrpt:  [344.63 Melem/s 346.85 Melem/s 348.69 Melem/s]

mono/sequential/lock/elems/4096
    time:   [38.026 µs 38.076 µs 38.134 µs]
    thrpt:  [107.41 Melem/s 107.58 Melem/s 107.72 Melem/s]

mono/sequential/atomic/elems/4096
    time:   [13.016 µs 13.055 µs 13.104 µs]
    thrpt:  [312.59 Melem/s 313.76 Melem/s 314.68 Melem/s]
```

And here's the equivalent theoretical maximum throughput in an async context
using `Tokio` and `Smol` runtimes:

```bash
mono/sequential/async/tokio/lock/elems/4096
    time:   [38.993 µs 39.033 µs 39.075 µs]
    thrpt:  [104.82 Melem/s 104.94 Melem/s 105.04 Melem/s]
mono/sequential/async/tokio/atomic/elems/4096
    time:   [22.046 µs 22.097 µs 22.171 µs]
    thrpt:  [184.74 Melem/s 185.36 Melem/s 185.80 Melem/s]

mono/sequential/async/smol/lock/elems/4096
    time:   [38.958 µs 39.085 µs 39.241 µs]
    thrpt:  [104.38 Melem/s 104.80 Melem/s 105.14 Melem/s]
mono/sequential/async/smol/atomic/elems/4096
    time:   [21.719 µs 21.864 µs 22.136 µs]
    thrpt:  [185.04 Melem/s 187.34 Melem/s 188.59 Melem/s]
```

To run all benchmarks:

```sh
cargo criterion --all-features
```

**NOTE**: Shared generators (like `LockSnowflakeGenerator` and
`AtomicSnowflakeGenerator`) can slow down under high thread contention. This
happens because threads must coordinate access - either through mutex locks or
atomic compare-and-swap (CAS) loops - which introduces overhead.

For maximum throughput, **avoid sharing**. Instead, give each thread its own
generator instance. This eliminates contention and allows every thread to issue
IDs independently at full speed.

The thread-safe generators are primarily for convenience, or for use cases where
ID generation is not expected to be the performance bottleneck.

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
