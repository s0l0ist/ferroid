# ferroid

High-throughput ULID and Snowflake-style IDs for Rust. Start with `ULID::now()`,
use thread-local generators for monotonic ULIDs or cached clock reads, or
configure a generator for Snowflake and custom IDs. Supports integer and Base32
representations, Serde, and `no_std`.

Requires Rust 1.99.1 or later.

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
# #[cfg(all(feature = "ulid", feature = "thread-rng"))]
# {
use ferroid::id::ULID;

// Read wall time and draw fresh randomness; IDs are not monotonic.
let id = ULID::now();
# }
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
# #[cfg(all(feature = "thread-local", not(all(target_arch = "wasm32", target_os = "unknown"))))]
# {
use ferroid::generator::thread_local::{cached, direct};

// Read wall time on each call: fresh randomness or increasing IDs per thread.
let random = direct::Ulid::new_ulid();
let increasing = direct::Ulid::new_ulid_mono(|_| std::thread::yield_now());

// Use a shared background clock cache for higher throughput.
let random = cached::Ulid::new_ulid();
let increasing = cached::Ulid::new_ulid_mono(|_| std::thread::yield_now());
# }
```

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
# #[cfg(all(feature = "snowflake", feature = "basic", feature = "std", not(all(target_arch = "wasm32", target_os = "unknown"))))]
# {
use ferroid::{
    generator::BasicSnowflakeGenerator,
    id::SnowflakeTwitterId,
    time::{CachedClock, TWITTER_EPOCH},
};

let clock = CachedClock::<1>::with_epoch(TWITTER_EPOCH); // Match the layout's epoch.
let generator = BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(1, clock);
let id = generator.next_id(|_| std::thread::yield_now());
# }
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
# #[cfg(all(feature = "snowflake", feature = "ulid"))]
# {
use ferroid::{define_snowflake_id, define_ulid};

define_snowflake_id!(
    EventId, u64,
    reserved: 1, timestamp: 41, machine_id: 10, sequence: 12
);

define_ulid!(
    TraceId, u128,
    reserved: 0, timestamp: 48, random: 80
);
# }
```

Fields must total the backing integer's width. Pass `EventId` or `TraceId` as the
ID type to an [explicit generator](#advanced-generators); generated types also
support the same Base32 methods and Serde adapters as built-in IDs.

IDs are copyable, sortable, and hashable. Use `to_raw()` / `from_raw()` for integer
storage, `timestamp()` for the timestamp field, and ULID's `system_time()` for a
`SystemTime` with `std`. See the [ID APIs][id-docs] for component access.

## Encoding and serialization

### Base32

Enable `base32` for Crockford Base32 formatting and validated decoding. Import
`Base32Ext` for built-in or custom IDs:

```rust
# #[cfg(all(feature = "ulid", feature = "base32"))]
# {
use ferroid::{base32::Base32Ext, id::ULID};

let id: ULID = "01ARZ3NDEKTSV4RRFFQ69G5FAV".parse().unwrap();
let encoded = id.encode(); // Fixed-size formatter; no heap allocation.
assert_eq!(ULID::decode(encoded.as_str()).unwrap(), id);
println!("{id}"); // Display uses the same encoding.
# assert_eq!(ULID::decode("ZZZZZZZZZZZZZZZZZZZZZZZZZZ").unwrap().encode().as_str(),
#     "7ZZZZZZZZZZZZZZZZZZZZZZZZZ");
# }
```

Canonical encodings preserve numeric sort order: 26 characters for a 128-bit
ULID and 13 for a 64-bit Snowflake. Use `encode_to_buf` to reuse a buffer, or
`to_string()` for an owned string.

**Decoding invariant:** for every built-in or custom ID type, Ferroid discards
encoded bits beyond the backing integer's width, then validates reserved layout
bits. Length and character checks still apply. Different strings can therefore
decode to the same ID; encoding the result produces its canonical representation.

This differs from the [ULID specification][ulid-spec], which rejects encodings
above `7ZZZZZZZZZZZZZZZZZZZZZZZZZ`. Ferroid accepts `ZZZZZZZZZZZZZZZZZZZZZZZZZZ`
for `ULID` and re-encodes it as `7ZZZZZZZZZZZZZZZZZZZZZZZZZ`. Reserved bits that
remain set after truncation cause `decode()` and `.parse()` to return
`base32::Error::DecodeOverflow`.

Use `MyId::try_from(raw)` or `raw.try_into()` for checked integer conversion.
Invalid layouts return `ferroid::Error::DecodeOverflow { id }`, preserving the
rejected ID.
`from_raw()` preserves all input bits without checking the layout. For IDs with
reserved bits, call `is_valid()` to check them or `into_valid()` to clear them,
which changes the ID's value. Macro-generated IDs expose both methods directly;
generic code uses the `Id` trait.

`from_components()` masks fields and leaves reserved bits zero. Oversized
Snowflake components and ULID timestamps panic with debug assertions enabled;
otherwise they are truncated. Excess ULID random bits are always discarded.
Validate untrusted components before construction: `is_valid()` cannot detect
bits that were already discarded.

The default `intrinsics` feature enables NEON on supported `AArch64` targets and
SSSE3 on `x86_64` targets built with `ssse3`. Selection is at compile time; 32-bit IDs
and other targets use scalar code. See the [Base32 reference][base32-docs].

### Serde

Enable `serde` and select either integer or Base32 representation on each field.
Base32 adapters also require `base32`; add Serde's derive support:

```toml
[dependencies]
ferroid = { version = "3", features = ["ulid", "snowflake", "serde", "base32"] }
serde = { version = "1", features = ["derive"] }
```

```rust
# #[cfg(all(feature = "ulid", feature = "snowflake", feature = "serde", feature = "base32"))]
# {
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
# let event = Event {
#     id: ULID::from_components(1_700_000_000_000, 42),
#     source: SnowflakeTwitterId::from_components(123_456, 1, 42),
# };
# let json = serde_json::to_string(&event).unwrap();
# let decoded: Event = serde_json::from_str(&json).unwrap();
# assert_eq!(decoded.id, event.id);
# assert_eq!(decoded.source, event.source);
# }
```

| Representation | Adapter                    | Required features |
| -------------- | -------------------------- | ----------------- |
| Integer        | `ferroid::serde::as_int`    | `serde`           |
| Base32 string  | `ferroid::serde::as_base32` | `serde`, `base32`  |

Both adapters work with `Id`, including macro-generated and manually implemented
IDs. Deserialization requires `TryFrom<ID::Ty, Error = ferroid::Error<ID>>`;
the layout macros implement this checked conversion automatically.
Base32 supports IDs backed by `u32`, `u64`, or `u128` through the sealed
`BeBytes` trait.

## Performance

Generation costs for the convenience methods above and Snowflake backends below:

| Method                             | Time per ID | IDs per second |
| ---------------------------------- | ----------- | -------------- |
| `ULID::now()`                      | ~36.5 ns    | ~27.4M         |
| `direct::Ulid::new_ulid()`         | ~38.2 ns    | ~26.1M         |
| `direct::Ulid::new_ulid_mono(...)` | ~28.7 ns    | ~34.9M         |
| `cached::Ulid::new_ulid()`         | ~15.5 ns    | ~64.4M         |
| `cached::Ulid::new_ulid_mono(...)` | ~3.54 ns    | ~283M          |
| `BasicSnowflakeGenerator`          | ~1.39 ns    | ~717M          |
| `AtomicSnowflakeGenerator`         | ~2.4 ns     | ~416M          |
| `LockSnowflakeGenerator`           | ~9.13 ns    | ~110M          |

Criterion medians on an Apple M1 Pro, macOS, Rust 1.100.0, with `all` and default
features enabled. Measurements use one thread after initialization and exclude
formatting. Snowflake rows use a cached clock and a custom layout with 22 sequence
bits to avoid exhaustion; each layout's sequence bits bound IDs per timestamp.
Results vary by platform and workload. See the [benchmark suite][benchmarks] to
measure your configuration.

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

Keep monotonic generator state alive across calls to preserve ordering. Atomic
ULID generators require a `u128` backing type and target 128-bit atomics.

**Snowflake generators** take `new(machine_id, clock)`:

| Snowflake Generator        | Monotonic | Thread-Safe | Lock-Free | Throughput | Use Case                                |
| -------------------------- | --------- | ----------- | --------- | ---------- | --------------------------------------- |
| `BasicSnowflakeGenerator`  | ✅        | ❌          | ❌        | Highest    | Single-threaded or generator per thread |
| `LockSnowflakeGenerator`   | ✅        | ✅          | ❌        | Medium     | Shared state protected by a mutex       |
| `AtomicSnowflakeGenerator` | ✅        | ✅          | ✅        | High       | Fast concurrent generation              |

Atomic Snowflake generators require `u64` and target 64-bit atomics. Basic and
lock generators support other backing widths. Lock-free describes generator
state updates; providers and callbacks may still block.

Standard mutex generators use `try_next_id` / `try_poll_id` to report poisoning;
`parking-lot` makes them infallible.

### Backoff and async generation

Snowflake and monotonic ULID generators invoke backoff when the sequence/random
field is exhausted or time moves backwards. Callbacks receive a suggested
`core::time::Duration`, with clock granularity already applied. It may be zero
after atomic contention. Returning from the callback retries generation.

For example, use the custom `TraceId` layout above with an owned generator:

```rust
# #[cfg(all(feature = "ulid", feature = "basic", feature = "thread-rng", not(all(target_arch = "wasm32", target_os = "unknown"))))]
# {
# ferroid::define_ulid!(TraceId, u128, reserved: 0, timestamp: 48, random: 80);
use ferroid::{
    generator::BasicMonoUlidGenerator, rand::ThreadRandom, time::DirectClock,
};

let generator = BasicMonoUlidGenerator::<TraceId, _, _>::new(
    DirectClock::default(), ThreadRandom,
);
let id = generator.next_id(std::thread::sleep);

// Or yield on immediate retries and sleep for clock waits.
let id = generator.next_id(|delay| {
    if delay.is_zero() {
        std::thread::yield_now();
    } else {
        std::thread::sleep(delay);
    }
});
# }
```

For manual retries, `poll_id()` / `try_poll_id()` return `Ready` or `Pending`,
with the same `Duration` in `Pending::yield_for`.

With `futures`, use the same generator inside an async context. For Tokio,
enable its `time` feature and pass its sleep function:

```rust ignore
use ferroid::futures::UlidGeneratorAsyncExt;

let id = generator.next_id_async(tokio::time::sleep).await;
```

With Smol, adapt its timer to a sleep callback returning `()`:

```rust ignore
let id = generator.next_id_async(|delay| async move {
    smol::Timer::after(delay).await;
}).await;
```

Snowflakes use `SnowflakeGeneratorAsyncExt` with the same calling pattern. Async
callbacks are awaited before retrying; fallible generators use `try_next_id_async`.

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

Only `intrinsics` is enabled by default. Use `all` for the functional APIs, or
select the features your application needs:

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
