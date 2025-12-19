# 🤖 ferroid

**ferroid** is a Rust library for generating and streaming time-sortable IDs,
including **Snowflake-style** IDs and **ULIDs**. It aims to be flexible while
having exceptional performance.

This workspace includes:

- [`ferroid`](./crates/ferroid): Core ID types and generators
- [`ferroid-tonic-core`](./crates/ferroid-tonic-core): gRPC protocol definitions
  and shared types
- [`ferroid-tonic-server`](./crates/ferroid-tonic-server): High-performance gRPC
  server that streams binary-packed ID chunks
- [`pg-ferroid`](./crates/pg-ferroid): A PostgreSQL extension for high-throughput
  ULID generation using ferroid

## 🚀 Getting Started

Run all tests

```bash
cargo test --features all
```

Run all benchmarks

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
