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

pub mod common;
pub mod server;
