# ferroid

[`ferroid`](https://github.com/s0l0ist/ferroid) is a Rust crate for
generating and parsing **Snowflake-style unique IDs**, compatible with public
formats used by platforms like Twitter, Discord, Instagram, and Mastodon. These
64-bit identifiers encode timestamps, machine/shard IDs, and sequence
numbers—making them lexicographically sortable, scalable, and ideal for
distributed systems.

This crate provides:

- 📌 Bit-level layout compatibility with major Snowflake formats
- 🧩 Pluggable time sources via the `TimeSource` trait
- 🧵 Lock & Lock-free and thread-safe ID generation
- 📐 Customizable layouts via the `Snowflake` trait
- 🔢 Lexicographically sortable string output

---

## 📦 Supported Layouts

| Platform  | Timestamp Bits | Machine ID Bits | Sequence Bits | Epoch                   |
| --------- | -------------- | --------------- | ------------- | ----------------------- |
| Twitter   | 41             | 10              | 12            | 2010-11-04 01:42:54.657 |
| Discord   | 42             | 10              | 12            | 2015-01-01 00:00:00.000 |
| Instagram | 41             | 13              | 10            | 2011-01-01 00:00:00.000 |
| Mastodon  | 48             | 0               | 16            | 1970-01-01 00:00:00.000 |

## 🔧 Generator Comparison

| Generator                  | Thread-Safe | Lock-Free | Throughput | Use Case                                 |
| -------------------------- | ----------- | --------- | ---------- | ---------------------------------------- |
| `BasicSnowflakeGenerator`  | ❌          | ❌        | Highest    | Single-threaded, one per thread          |
| `LockSnowflakeGenerator`   | ✅          | ❌        | Medium     | Multi-threaded, high contention          |
| `AtomicSnowflakeGenerator` | ✅          | ✅        | Medium     | Multi-threaded, low-to-medium contention |

All generators produce **monotonically increasing**, **time-ordered**, and
**unique** IDs.

---

## 🚀 Usage

### Generate an ID

```rust
use ferroid::{MonotonicClock, TWITTER_EPOCH, BasicSnowflakeGenerator, SnowflakeTwitterId, IdGenStatus};

let clock = MonotonicClock::with_epoch(TWITTER_EPOCH);
let mut generator = BasicSnowflakeGenerator::<_, SnowflakeTwitterId>::new(1, clock);

let id = loop {
    match generator.next_id() {
        IdGenStatus::Ready { id } => break id,
        IdGenStatus::Pending { yield_until } => {
            println!("Exhausted; wait until: {}", yield_until);
            std::hint::spin_loop();
        }
    }
};

println!("Generated ID: {}", id);
```

### Behavior

- If the clock **advances**: reset sequence to 0 → `IdGenStatus::Ready`
- If the clock is **unchanged**: increment sequence → `IdGenStatus::Ready`
- If the clock **goes backward**: return `IdGenStatus::Pending`
- If the sequence **overflows**: return `IdGenStatus::Pending`

### Serialize as padded string

```rust
use ferroid::{SnowflakeTwitterId};

let id = SnowflakeTwitterId::from(123456, 1, 42);
println!("id: {id}");
// > id: 517811998762
println!("id padded: {}", id.to_padded_string());
// > id padded: 00000000517811998762

// Crockford base32
let encoded = id.encode();
println!("encoded: {encoded}");
// > encoded: 00000Y4G0082M

// Decode from Base32
let decoded = SnowflakeTwitterId::decode(&encoded).expect("decode should succeed");

assert_eq!(id, decoded);
```

## 📈 Benchmarks

`ferroid` ships with Criterion benchmarks to measure ID generation
performance:

- `BasicSnowflakeGenerator`: single-threaded generator
- `LockSnowflakeGenerator`: mutex-based, thread-safe generator
- `AtomicSnowflakeGenerator`: lock-free, thread-safe generator

Benchmark scenarios include:

- Generating IDs from a single thread with a mock clock
- Generating IDs from a single thread with a real clock
- Generating IDs from multiple threads with a mock clock
- Generating IDs from multiple threads with a real clock

**NOTE**: Generators may perform worse under multithreaded contention due to
locking or atomic compare-and-swap (CAS) overhead. For maximum throughput,
assign a separate generator to each thread and avoid contention entirely.

To run:

```sh
cargo criterion
```

## 🧪 Testing

Run with:

```sh
cargo test --all-features
```

## 📄 License

Licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- [MIT License](LICENSE-MIT or <https://opensource.org/licenses/MIT>)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
