# Shared Types and Protobuf Bindings

This crate provides the shared interface and protocol definitions for the
`ferroid` streaming ID generation ecosystem. It is consumed by both the gRPC
server and external clients that interact with it programmatically.

## Contents

- Canonical Snowflake ID types and encoding constants
- Shared [`Error`] and [`Result`] types used across the system
- Auto-generated protobuf bindings for the gRPC interface (`ferroid.proto`)

## Protobuf Interface

The [`proto`] module contains types generated via [`tonic::include_proto!`]. It
also exposes a precompiled `FILE_DESCRIPTOR_SET` for gRPC reflection support.

Example client usage:

```rust
use ferroid_tonic_core::proto::id_generator_client::IdGeneratorClient;
```

gRPC clients should deserialize `IdChunk.packed_ids` using the agreed-upon
fixed-width layout (e.g. `u64` or `u128`, in little-endian order).
