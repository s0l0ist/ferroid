# 🧊 ferroid

**ferroid** is a modular Rust workspace for generating and streaming
time-sortable IDs - including Snowflake-style IDs and ULIDs - with optional
high-throughput gRPC support.

This workspace includes:

- `ferroid`: Core ID logic and generators
- `ferroid-tonic-core`: gRPC protocol definitions and shared types
- `ferroid-tonic-server`: A gRPC server that streams binary packed ID chunks
  over a network

## 📦 Workspace Structure

### `crates/ferroid`

The core library. Provides:

- ✅ **ID Types**: Snowflake (`u64`), ULID (`u128`), or any custom variant.
- ⚙️ **Generators**:

  - `BasicSnowflakeGenerator`: single-threaded, lock-free
  - `LockSnowflakeGenerator`: multi-threaded with locking
  - `AtomicSnowflakeGenerator`: multi-threaded, lock-free
  - `BasicUlidGenerator`: high-entropy ULID generation

- 🔁 **Async Integration**: support via `tokio`, `smol`, and sleep traits
- 🔡 **Encoding Support**: base32 encoding/decoding for sortable string IDs

This is the crate you'll typically depend on for ID generation.

### `crates/ferroid-tonic-core`

Defines the gRPC protocol and shared types for ID streaming:

- 📜 `ferroid.proto` for ID stream requests and packed binary responses
- 📦 Shared types used by both client and server
- 🤝 Ensures type compatibility across deployments

Note: The server and client should be compiled with the same
`ferroid-tonic-core`. If you're overriding the default ID
(`SnowflakeTwitterId`), please fork this repo to ensure contract stability
between client and server.

### `crates/ferroid-tonic-server`

A gRPC server for streaming IDs:

- 📶 Supports streaming chunked IDs
- 🧵 Concurrent worker task pool with backpressure
- 🛑 Graceful shutdown and stream cancellation
- 🗂️ Optional compression (`zstd`, `gzip`, `deflate`)
- 📊 OpenTelemetry metrics and tracing

Run this to expose high-throughput ID generation as a network service.

```bash
cargo run -p ferroid-tonic-server --features tracing
```

## 🚀 Getting Started

Run tests across all crates

```bash
cargo test --all-features
```

Run benchmarks across all crates

```bash
cargo criterion --all-features
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
