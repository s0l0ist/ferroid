//! Streaming orchestration for chunked Snowflake ID generation.
//!
//! This module implements the end-to-end logic for fulfilling streaming ID
//! generation requests. It handles chunking, cancellation, and forwarding of
//! binary-packed ID chunks back to the gRPC client.
//!
//! ## Submodules
//!
//! - [`coordinator`] - Splits large requests into chunks and routes them to the
//!   worker pool.
//! - [`processor`] - Performs low-level ID generation and chunk packing inside
//!   workers.
//! - [`request`] - Defines the [`WorkRequest`] enum used to communicate between
//!   the dispatcher and workers.
//!
//! These components work together to ensure high-throughput,
//! backpressure-aware, cancellable streams of Snowflake IDs from server to
//! client.

pub mod coordinator;
pub mod processor;
pub mod request;
