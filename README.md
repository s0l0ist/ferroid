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

Calling `next_id()` may yield `Pending` if the current sequence is exhausted. In
that case, you can spin, yield, or sleep depending on your environment:

```rust
use ferroid::{MonotonicClock, TWITTER_EPOCH, BasicSnowflakeGenerator, SnowflakeTwitterId, IdGenStatus};

let clock = MonotonicClock::with_epoch(TWITTER_EPOCH);
let mut generator = BasicSnowflakeGenerator::<_, SnowflakeTwitterId>::new(1, clock);

let id: SnowflakeTwitterId = loop {
    match generator.next_id() {
        IdGenStatus::Ready { id } => break id,
        IdGenStatus::Pending { yield_until } => {
            println!("Exhausted; wait until: {}", yield_until);
            std::hint::spin_loop();
            // Use `std::hint::spin_loop()` for single-threaded or per-thread generators.
            // Use `std::thread::yield_now()` when sharing a generator across multiple threads.
            // Use `tokio::time::sleep().await` in async contexts (e.g., Tokio thread pool).
        }
    }
};

println!("Generated ID: {}", id);
```

Or use another pre-built layout such as `Mastodon`:

```rust
use ferroid::{MonotonicClock, MASTODON_EPOCH, BasicSnowflakeGenerator, SnowflakeMastodonId, IdGenStatus};

let clock = MonotonicClock::with_epoch(MASTODON_EPOCH);
let mut generator = BasicSnowflakeGenerator::<_, SnowflakeMastodonId>::new(1, clock);

// loop as above
```

### Custom Layouts

To define a custom Snowflake layout, implement `Snowflake` and optionally
`Base32`:

```rust
use ferroid::{Snowflake, Base32};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct MyCustomId {
    id: u64,
}

// required
impl Snowflake for MyCustomId {
    // impl required methods
}

// optional, only if you need it
impl Base32 for MyCustomId {}
```

### Behavior

- If the clock **advances**: reset sequence to 0 → `IdGenStatus::Ready`
- If the clock is **unchanged**: increment sequence → `IdGenStatus::Ready`
- If the clock **goes backward**: return `IdGenStatus::Pending`
- If the sequence **overflows**: return `IdGenStatus::Pending`

### Serialize as padded string

Use `.to_padded_string()` or `.encode()` for sortable representations:

```rust
use ferroid::{SnowflakeTwitterId};

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

`ferroid` ships with Criterion benchmarks to measure ID generation performance:

- `BasicSnowflakeGenerator`: single-threaded generator
- `LockSnowflakeGenerator`: mutex-based, thread-safe generator
- `AtomicSnowflakeGenerator`: lock-free, thread-safe generator

Benchmark scenarios include:

- Single-threaded with/without a real clock
- Multi-threaded with/without a real clock

**NOTE**: Shared generators (like `LockSnowflakeGenerator` and
`AtomicSnowflakeGenerator`) can slow down under high thread contention. This
happens because threads must coordinate access - either through mutex locks or
atomic compare-and-swap (CAS) loops - which introduces overhead.

For maximum throughput, **avoid sharing**. Instead, give each thread its own
generator instance. This eliminates contention and allows every thread to issue
IDs independently at full speed.

The thread-safe generators are primarily for convenience, or for use cases where
ID generation is not expected to be the performance bottleneck. To run:

```sh
cargo criterion
```

## 🧪 Testing

Run all tests with:

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
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
