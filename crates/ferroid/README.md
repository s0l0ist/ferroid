# ferroid

High-throughput ULID and Snowflake-style IDs for Rust. Start with `ULID::now()`,
use thread-local generators for monotonic ULIDs or cached clock reads, or
configure a generator for Snowflake and custom IDs. Supports integer and Base32
representations, Serde, and `no_std`.

Requires Rust 1.95.0 or later.

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

[API documentation][api] · [Changelog][changelog] · [Benchmarks][benchmarks]

## Contents

- [Generating IDs](#generating-ids)
  - [ULIDs](#ulids)
  - [Thread-local ULIDs](#thread-local-ulids)
  - [Snowflake IDs](#snowflake-ids)
  - [Custom ID layouts](#custom-id-layouts)
- [Encoding and serialization](#encoding-and-serialization)
  - [Base32](#base32)
  - [Serde](#serde)
- [Performance](#performance)
  - [ID generation](#id-generation)
  - [Base32 with intrinsics enabled](#base32-with-intrinsics-enabled)
- [Advanced generators](#advanced-generators)
  - [Choosing a generator](#choosing-a-generator)
  - [Backoff and async generation](#backoff-and-async-generation)
  - [Clocks and random sources](#clocks-and-random-sources)
- [Features and platforms](#features-and-platforms)
  - [Feature flags](#feature-flags)
  - [`no_std`](#no_std)
  - [WASM and browsers](#wasm-and-browsers)
- [License](#license)

## Generating IDs

### ULIDs

`id::ULID` combines a 48-bit Unix-millisecond timestamp with 80 random bits.
For non-monotonic IDs, use `ULID::now()`:

```toml
[dependencies]
ferroid = { version = "3", features = ["ulid", "thread-rng"] }
```

```rust
use ferroid::id::ULID;

// Read wall time and draw fresh randomness; IDs are not monotonic.
let id = ULID::now();
```

### Thread-local ULIDs

Use `direct::Ulid` for monotonic generation per thread, or `cached::Ulid` for
a shared background clock cache. Both return `id::ULID` values; the direct
non-monotonic method delegates to `ULID::now()`:

```toml
[dependencies]
ferroid = { version = "3", features = ["thread-local"] }
```

```rust
use ferroid::generator::thread_local::{cached, direct};

// Read wall time on each call: fresh randomness or increasing IDs per thread.
let random = direct::Ulid::new_ulid();
let increasing = direct::Ulid::new_ulid_mono(|_| std::thread::yield_now());

// Use a shared background clock cache for higher throughput.
let random = cached::Ulid::new_ulid();
let increasing = cached::Ulid::new_ulid_mono(std::thread::sleep);
```

Backoff can yield immediately with `|_| std::thread::yield_now()` or sleep for
the suggested duration with `std::thread::sleep`.

| Method                             | Clock           | Randomness       | Monotonic | Background thread | Throughput |
| ---------------------------------- | --------------- | ---------------- | --------- | ----------------- | ---------- |
| `ULID::now()`                      | Wall-clock read | Every call       | ❌        | ❌                | Slow       |
| `direct::Ulid::new_ulid()`         | Wall-clock read | Every call       | ❌        | ❌                | Slow       |
| `direct::Ulid::new_ulid_mono(...)` | Wall-clock read | On new timestamp | ✅        | ❌                | Medium     |
| `cached::Ulid::new_ulid()`         | Cached read     | Every call       | ❌        | ✅                | Fast       |
| `cached::Ulid::new_ulid_mono(...)` | Cached read     | On new timestamp | ✅        | ✅                | Fastest    |

Throughput ratings describe steady-state use and depend on the clock, hardware,
and RNG. The cached methods start one process-wide background clock thread on
first use; it refreshes roughly once per millisecond, subject to scheduling.

Monotonic methods guarantee increasing IDs **per thread**, not across threads.
They increment the random field within a timestamp and invoke the callback when
that field is exhausted or the clock moves backwards. Independent generators
still have a [collision probability][collisions].

### Snowflake IDs

Snowflakes pack a timestamp, machine ID, and sequence into an integer. Create a
generator with the layout's matching epoch and a unique machine ID:

```toml
[dependencies]
ferroid = { version = "3", features = ["snowflake", "basic", "std"] }
```

```rust
use ferroid::{
    generator::BasicSnowflakeGenerator,
    id::SnowflakeTwitterId,
    time::{CachedClock, TWITTER_EPOCH},
};

let clock = CachedClock::<1>::with_epoch(TWITTER_EPOCH); // Match the layout's epoch.
let generator = BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(1, clock);
let id = generator.next_id(|_| std::thread::yield_now());
```

Keep the generator alive and assign each independent generator a unique machine
ID. Recreating one with the same machine ID in the same timestamp can repeat IDs.
For shared state, see [Choosing a generator](#choosing-a-generator).

Built-in layouts are 64-bit values:

| ID type                | Timestamp bits | Machine bits | Sequence bits | Epoch constant    |
| ---------------------- | -------------- | ------------ | ------------- | ----------------- |
| `SnowflakeTwitterId`   | 41             | 10           | 12            | `TWITTER_EPOCH`   |
| `SnowflakeDiscordId`   | 42             | 10           | 12            | `DISCORD_EPOCH`   |
| `SnowflakeInstagramId` | 41             | 13           | 10            | `INSTAGRAM_EPOCH` |
| `SnowflakeMastodonId`  | 48             | 0            | 16            | `MASTODON_EPOCH`  |

Twitter reserves its highest bit; the other layouts use all 64 bits. The Mastodon
layout has no machine bits and requires coordination between independent
generators. Epochs are configured on clocks, not stored in ID types.

### Custom ID layouts

Define your own layout with the corresponding ID family enabled:

```rust
use ferroid::{define_snowflake_id, define_ulid};

define_snowflake_id!(
    EventId, u64,
    reserved: 1, timestamp: 41, machine_id: 10, sequence: 12
);

define_ulid!(
    TraceId, u128,
    reserved: 0, timestamp: 48, random: 80
);
```

Fields must total the backing integer's width. Pass `EventId` or `TraceId` as the
ID type to an [explicit generator](#advanced-generators); generated types also
support the same Base32 methods and Serde adapters as built-in IDs.

IDs are copyable, sortable, and hashable. Use `to_raw()` / `from_raw()` for integer
storage, `timestamp()` for the timestamp field, and ULID's `system_time()` for a
`SystemTime` with `std`. See the [ID APIs][id-docs] for component access.

## Encoding and serialization

### Base32

Enable `base32` for Crockford Base32 encoding and decoding:

```rust
use ferroid::{base32::Base32Ext, id::ULID};

let id: ULID = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
let encoded = id.encode(); // Stack buffer; no allocation.
assert_eq!(ULID::decode(encoded.as_str()).unwrap(), id);
```

Encodings are fixed-width: 7 characters for `u32`, 13 for `u64`, and 26 for
`u128`. Canonical strings of the same width preserve numeric order when sorted
in ASCII order. Decoding accepts lowercase and Crockford aliases; re-encoding
produces uppercase canonical text.

The [ULID specification][ulid-spec] states:

> Any attempt to decode or encode a ULID larger than this should be rejected by
> all implementations, to prevent overflow bugs.

Here, "this" is the maximum ULID encoding, `7ZZZZZZZZZZZZZZZZZZZZZZZZZ`.

**Ferroid handles overflow differently.** It discards
encoded high bits beyond the backing integer's width before validating the ID.
For example, `ULID::decode("ZZZZZZZZZZZZZZZZZZZZZZZZZZ")` succeeds and re-encodes
as `7ZZZZZZZZZZZZZZZZZZZZZZZZZ`; the specification requires rejecting that input.
Incorrect lengths, invalid characters, and nonzero reserved layout bits are
rejected for built-in and macro-generated IDs. Encoding does not validate the ID.

The default `intrinsics` feature selects NEON on `AArch64` builds with `neon`, or
SSSE3 on x86-64 builds with `ssse3`, for 64- and 128-bit IDs. Other configurations
use scalar code. See the [Base32 reference][base32-docs] for buffer reuse and
conversion methods.

### Serde

Enable `serde` and select either integer or Base32 representation on each field.
Base32 adapters also require `base32`; add Serde's derive support:

```toml
[dependencies]
ferroid = { version = "3", features = ["ulid", "snowflake", "serde", "base32"] }
serde = { version = "1", features = ["derive"] }
```

```rust
use ferroid::id::{SnowflakeTwitterId, ULID};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct Event {
    // Encode the ULID as a Base32 string.
    #[serde(with = "ferroid::serde::as_base32")]
    id: ULID,

    // Encode the Snowflake as its backing integer.
    #[serde(with = "ferroid::serde::as_int")]
    source: SnowflakeTwitterId,
}
```

| Representation | Adapter                    | Required features |
| -------------- | -------------------------- | ----------------- |
| Integer        | `ferroid::serde::as_int`    | `serde`           |
| Base32 string  | `ferroid::serde::as_base32` | `serde`, `base32`  |

## Performance

Measured on an Apple M1 Pro, macOS, Rust 1.100.0. Values below are rounded
Criterion point estimates from the latest run. See the [benchmark suite][benchmarks].

### ID generation

Generation costs after initialization, excluding formatting. These measurements
use one calling thread; shared generators are measured without contention.

| Method                             | Clock  | Time per ID | IDs per second |
| ---------------------------------- | ------ | ----------- | -------------- |
| `ULID::now()`                      | Direct | ~35.9 ns    | ~27.8M         |
| `direct::Ulid::new_ulid()`         | Direct | ~35.4 ns    | ~28.2M         |
| `direct::Ulid::new_ulid_mono(...)` | Direct | ~28.2 ns    | ~35.4M         |
| `cached::Ulid::new_ulid()`         | Cached | ~15.0 ns    | ~66.5M         |
| `cached::Ulid::new_ulid_mono(...)` | Cached | ~3.45 ns    | ~290M          |
| `AtomicMonoUlidGenerator`          | Cached | ~5.15 ns    | ~194M          |
| `LockMonoUlidGenerator`            | Cached | ~8.95 ns    | ~112M          |
| `BasicSnowflakeGenerator`          | Cached | ~1.35 ns    | ~741M          |
| `AtomicSnowflakeGenerator`         | Cached | ~2.33 ns    | ~428M          |
| `LockSnowflakeGenerator`           | Cached | ~8.85 ns    | ~113M          |

Snowflake measurements use a cached clock and a layout with 22 sequence bits.

### Base32 with intrinsics enabled

Base32 conversion using NEON with the default `intrinsics` feature enabled:

| Operation                | Snowflake (`u64`, 13 characters) | ULID (`u128`, 26 characters) |
| ------------------------ | -------------------------------- | ---------------------------- |
| `encode_to_buf()`        | ~1.30 ns                         | ~1.58 ns                     |
| `encode().as_str()`      | ~1.28 ns                         | ~1.54 ns                     |
| `encode().as_string()`   | ~13.53 ns                        | ~13.61 ns                    |
| `decode()`               | ~1.62 ns                         | ~2.35 ns                     |

## Advanced generators

### Choosing a generator

Use a generator explicitly for custom ID layouts, clocks or RNGs, shared state,
or async backoff. Thread-safety ratings assume thread-safe providers; throughput
ratings assume comparable providers and no contention.

**ULID generators** take `new(clock, rng)`:

| ULID Generator            | Monotonic | Thread-Safe | Lock-Free | Throughput | Use Case                                |
| ------------------------- | --------- | ----------- | --------- | ---------- | --------------------------------------- |
| `BasicUlidGenerator`      | ❌        | ✅          | ❌        | Slow       | Fresh randomness on every call          |
| `BasicMonoUlidGenerator`  | ✅        | ❌          | ❌        | Highest    | Single-threaded or generator per thread |
| `LockMonoUlidGenerator`   | ✅        | ✅          | ❌        | Medium     | Shared state protected by a mutex       |
| `AtomicMonoUlidGenerator` | ✅        | ✅          | ✅        | High       | Fast concurrent generation              |

**Snowflake generators** take `new(machine_id, clock)`:

| Snowflake Generator        | Monotonic | Thread-Safe | Lock-Free | Throughput | Use Case                                |
| -------------------------- | --------- | ----------- | --------- | ---------- | --------------------------------------- |
| `BasicSnowflakeGenerator`  | ✅        | ❌          | ❌        | Highest    | Single-threaded or generator per thread |
| `LockSnowflakeGenerator`   | ✅        | ✅          | ❌        | Medium     | Shared state protected by a mutex       |
| `AtomicSnowflakeGenerator` | ✅        | ✅          | ✅        | High       | Fast concurrent generation              |

### Backoff and async generation

Snowflake and monotonic ULID generators call backoff when time moves backwards
or the sequence/random field is exhausted. The callback receives a `Duration`;
returning from it retries generation. Use `std::thread::sleep` for timed backoff:

```rust
use ferroid::{
    generator::BasicMonoUlidGenerator, id::ULID, rand::ThreadRandom, time::DirectClock,
};

let generator = BasicMonoUlidGenerator::<ULID, _, _>::new(DirectClock::default(), ThreadRandom);
let id = generator.next_id(std::thread::sleep);

// Or yield without waiting for the suggested duration.
let id = generator.next_id(|_| std::thread::yield_now());
```

With `futures`, pass your runtime's async sleep function. For Tokio, enable its
`time` feature:

```rust ignore
use ferroid::futures::UlidGeneratorAsyncExt;

let id = generator.next_id_async(tokio::time::sleep).await;
```

With Smol:

```rust ignore
let id = generator.next_id_async(|delay| async move {
    smol::Timer::after(delay).await;
}).await;
```

Snowflakes use `SnowflakeGeneratorAsyncExt`. See the [generator reference][generator-docs]
for fallible generation, manual polling, and backoff strategies.

### Clocks and random sources

| Clock         | Time reads                                             | Background thread |
| ------------- | ------------------------------------------------------ | ----------------- |
| `DirectClock` | Wall time on each call                                 | ❌                |
| `CachedClock` | A shared timestamp refreshed roughly every millisecond | ✅                |

Both clocks follow system clock corrections, default to the Unix epoch, and
support `with_epoch`. `DirectClock<N>` and `CachedClock<N>` can use coarser
`N`-millisecond ticks; keep `N = 1` for standard ID formats.

ULID generators accept `ThreadRandom` or a custom RNG. Implement
[`TimeSource`][time-source] and [`RandSource`][rand-source] for the ID's backing
integer to supply your own clock and randomness. Their rustdoc covers custom
implementations.

## Features and platforms

### Feature flags

Enable at least one ID family: `ulid` or `snowflake`. Only `intrinsics` is
enabled by default. Use `all` for the functional APIs, or select the features
your application needs:

| Feature                   | Provides                                                                          |
| ------------------------- | --------------------------------------------------------------------------------- |
| `ulid`, `snowflake`       | ID types and layout macros for that family                                        |
| `basic`, `atomic`, `lock` | Generator implementations; `lock` includes `std` and `alloc`                      |
| `thread-local`            | ULID facades; includes `ulid`, `basic`, `thread-rng`, and `alloc`                 |
| `thread-rng`              | `ThreadRandom` and random convenience constructors; includes `std`                |
| `std`                     | Built-in clocks and standard time conversions                                     |
| `alloc`                   | Owned string helpers such as `encode().as_string()` and `String::from(id)`        |
| `base32`                  | Base32 formatting and parsing                                                     |
| `serde`                   | Serialization adapters                                                            |
| `futures`                 | Async generator extensions without a runtime dependency                           |
| `intrinsics`              | Target-specific Base32 acceleration                                               |
| `parking-lot`             | Non-poisoning mutex backend; includes `lock`                                      |
| `tracing`                 | Generator tracing spans                                                           |
| `all`                     | Functional APIs; does not itself enable `intrinsics`, `parking-lot`, or `tracing` |

### `no_std`

ID types, basic and supported atomic generators, Base32, Serde adapters, and
async extensions work without `std` or allocation. Supply your platform's
`TimeSource` and `RandSource`; enable `alloc` separately for owned string helpers.
`std` and `alloc` are independent features.

### WASM and browsers

For occasional browser IDs, use `ULID::now()`. Enable `ulid` and `thread-rng`,
and configure JavaScript entropy in the application:

```toml
[dependencies]
ferroid = { version = "3", features = ["ulid", "thread-rng"] }
getrandom = { version = "0.4", features = ["wasm_js"] }
```

`std` automatically selects `web-time` on `wasm32-unknown-unknown`.
`direct::Ulid` also works with `thread-local` enabled; its state is local to
the WASM instance. `cached::Ulid` and `CachedClock` are unavailable because
they require a background thread.

## License

Licensed under either of:

- [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
  ([LICENSE-APACHE][apache-url])
- [MIT License](https://opensource.org/licenses/MIT)
  ([LICENSE-MIT][mit-url])

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

[api]: https://docs.rs/ferroid/latest/ferroid/
[ulid-spec]: https://github.com/ulid/spec#overflow-errors-when-parsing-base32-strings
[base32-docs]: https://docs.rs/ferroid/latest/ferroid/base32/index.html
[collisions]: https://github.com/s0l0ist/ferroid/blob/main/crates/ferroid/docs/collision-analysis.md
[changelog]: https://github.com/s0l0ist/ferroid/blob/main/CHANGELOG.md
[benchmarks]: https://github.com/s0l0ist/ferroid/tree/main/crates/ferroid-benches
[id-docs]: https://docs.rs/ferroid/latest/ferroid/id/index.html
[time-source]: https://docs.rs/ferroid/latest/ferroid/time/trait.TimeSource.html
[rand-source]: https://docs.rs/ferroid/latest/ferroid/rand/trait.RandSource.html
[generator-docs]: https://docs.rs/ferroid/latest/ferroid/generator/index.html
