//! # ferroid-tonic Server
//!
//! This binary launches a high-performance, gRPC-based Snowflake ID generation
//! service using [`ferroid`] for ID generation and [`tonic`] for gRPC
//! infrastructure.
//!
//! The server provides single and streaming endpoints for clients to request
//! batches of Snowflake-like unique IDs. It is designed to handle massive
//! throughput with backpressure, cancellation, and compression support.
//!
//! ## Features
//!
//! - Streamed or single-response ID generation.
//! - Fully asynchronous, backpressure-aware worker pool.
//! - Graceful shutdown with signal handling.
//! - Adaptive HTTP/2 flow control for high-throughput gRPC traffic.
//! - Compressed responses with Zstd support.
//!
//! ## Example
//!
//! ```bash
//! cargo run --bin server --release
//! ```

pub mod common;
pub mod server;
pub mod idgen {
    tonic::include_proto!("idgen");
}
