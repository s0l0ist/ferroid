//! # `ferroid-tonic`: Streaming Snowflake ID Generation Service
//!
//! `ferroid-tonic` is a high-performance, gRPC-based Snowflake ID generation
//! service built on top of [`ferroid`] for timestamp-based ID generation and
//! [`tonic`] for HTTP/2 transport.
//!
//! This crate powers a standalone server that accepts streaming requests for
//! batches of Snowflake-like IDs. It is designed for workloads that demand:
//!
//! - Time-ordered, collision-free IDs
//! - Large batch throughput
//! - Efficient use of memory and network bandwidth
//!
//! ## Highlights
//!
//! - **Streaming-only gRPC Endpoint**: Clients stream requests for up to
//!   billions of IDs per call, delivered in compressed, chunked responses.
//! - **Tokio Worker Pool**: Each worker runs a dedicated ID generator with
//!   bounded task queues.
//! - **Backpressure Aware**: All queues are size-limited to avoid memory
//!   blowup.
//! - **Client Cancellation**: Streamed requests are cancellable mid-flight.
//! - **Graceful Shutdown**: Coordinated shutdown ensures no work is lost.
//! - **Zstd Compression**: gRPC chunks are compressed for efficient transfer.
//!
//! ## Usage
//!
//! Build and run the gRPC server with:
//!
//! ```bash
//! cargo run --bin server --release
//! ```
//!
//! Then connect via gRPC on `127.0.0.1:50051` using the `GetStreamIds`
//! endpoint.
//!
//! ## Module Overview
//!
//! - [`common`] - Shared types and error definitions.
//! - [`server`] - gRPC service implementation, worker orchestration, and
//!   streaming logic.
//! - [`idgen`] - Generated Protobuf service and message definitions.
//!
//! ## Related Crates
//!
//! - [`ferroid`](https://crates.io/crates/ferroid): Embedded Snowflake
//!   generator.
//! - [`tonic`](https://crates.io/crates/tonic): gRPC transport over HTTP/2.

pub mod common;
pub mod server;

/// gRPC service and message definitions generated from `proto/idgen.proto`.
///
/// This module defines the streaming API for high-throughput ID generation.
///
/// ## Service
///
/// - [`GetStreamIds`] - Streams back a client-specified number of unique IDs as
///   binary chunks.
///
/// ## Message Format
///
/// - [`IdStreamRequest`] - Specifies how many IDs to generate.
/// - [`IdUnitResponseChunk`] - Contains a `packed_ids` byte buffer with
///   serialized Snowflake IDs.
///
/// The server uses a raw binary format for performance:
/// - IDs are fixed-width (typically 8 or 16 bytes).
/// - Encoded in little-endian order.
/// - Packed into a contiguous `bytes` buffer per chunk.
///
/// ### Example Client Deserialization (Rust)
/// ```rust
/// use ferroid::{Snowflake, SnowflakeTwitterId};
///
/// for chunk in response_stream {
///     for bytes in chunk.packed_ids.chunks_exact(size_of::<<SnowflakeTwitterId as Snowflake>::Ty>()) {
///         let raw_id = u64::from_le_bytes(bytes.try_into().unwrap());
///          let id = SnowflakeTwitterId::from_raw(raw_id);
///         // use ID...
///     }
/// }
/// ```
///
/// Clients must match their decode width (`u64`, `u128`) with the server
/// configuration.
///
/// ## Invariants
/// - `packed_ids.len() % ID_SIZE == 0`
/// - Each chunk contains ≥ 1 complete ID
///
/// See `proto/idgen.proto` for full schema and comments.
///
/// [`GetStreamIds`]: [`crate::idgen::GetStreamIds`]
/// [`IdStreamRequest`]: [`crate::idgen::IdStreamRequest`]
/// [`IdUnitResponseChunk`]: [`crate::idgen::IdUnitResponseChunk`]
pub mod idgen {
    tonic::include_proto!("idgen");
}
