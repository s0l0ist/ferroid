//! System-wide configuration constants and type definitions for the ID
//! generation service.
//!
//! This module centralizes all tuning parameters related to concurrency,
//! buffering, chunking, and Snowflake ID generation behavior. It also provides
//! type aliases for key generator traits and implementations.
//!
//! ## Key Concepts
//!
//! - **Worker Pool Sizing**: The number of worker tasks is controlled via
//!   [`NUM_WORKERS`]. Each worker is uniquely assigned a Snowflake machine ID.
//!
//! - **Backpressure Tuning**: Bounded channels are used throughout the system
//!   to prevent memory blowup under load. [`DEFAULT_WORK_REQUEST_BUFFER_SIZE`]
//!   and [`DEFAULT_STREAM_BUFFER_SIZE`] control the depth of these buffers.
//!
//! - **Chunking Strategy**: Snowflake IDs are grouped into fixed-size binary
//!   chunks before being sent over the network, improving throughput and memory
//!   efficiency. This is governed by [`DEFAULT_IDS_PER_CHUNK`] and
//!   [`DEFAULT_CHUNK_BYTES`].
//!
//! - **ID Space Safety**: A compile-time assertion ensures that the number of
//!   workers does not exceed the Snowflake machine ID bit width, preventing ID
//!   collisions in distributed deployments.
//!
//! ## Type Aliases
//!
//! - [`ClockType`]: The clock source used by all Snowflake generators.
//! - [`SnowflakeGeneratorType`]: The specific generator implementation assigned
//!       to each worker.

use crate::common::types::{SNOWFLAKE_ID_SIZE, SnowflakeIdType};
use ferroid::{BasicSnowflakeGenerator, MonotonicClock};

/// Offset used to shard worker IDs in multi-cluster or multi-tenant
/// environments.
///
/// This can be set to avoid overlap in machine ID space when multiple clusters
/// share a global ID namespace.
pub const SHARD_OFFSET: usize = 0;

/// Total number of concurrent ID generation workers.
///
/// Each worker is assigned a unique machine ID derived from its index +
/// [`SHARD_OFFSET`]. This value must not exceed the addressable space defined
/// by the Snowflake ID format.
pub const NUM_WORKERS: usize = 512;

/// Compile-time safety check: ensure number of workers does not exceed machine
/// ID space.
///
/// Prevents ID collisions by verifying that the number of distinct machine IDs
/// (workers) fits within the Snowflake format's machine ID bit width.
const _: () = assert!(
    NUM_WORKERS <= (SnowflakeIdType::max_machine_id() as usize) + 1,
    "NUM_WORKERS exceeds available Snowflake machine ID space"
);

/// Upper limit for the number of IDs allowed in a single `GetStreamIds`
/// request.
///
/// Large requests are split into smaller chunked sub-requests for processing.
pub const MAX_ALLOWED_IDS: usize = 1_000_000_000;

/// Capacity of the per-worker bounded MPSC work queue.
///
/// Controls how many `WorkRequest`s can be buffered before backpressure is
/// applied.
pub const DEFAULT_WORK_REQUEST_BUFFER_SIZE: usize = 2048;

/// Number of Snowflake IDs packed into each response chunk.
///
/// This affects memory footprint per chunk and network efficiency.
pub const DEFAULT_IDS_PER_CHUNK: usize = 2048;

/// Total size in bytes of a packed ID chunk.
///
/// Calculated as `DEFAULT_IDS_PER_CHUNK * SNOWFLAKE_ID_SIZE`.
pub const DEFAULT_CHUNK_BYTES: usize = DEFAULT_IDS_PER_CHUNK * SNOWFLAKE_ID_SIZE;

/// Capacity of the gRPC response channel between the worker and the client
/// stream.
///
/// Affects how aggressively the server can pipeline ID chunks to the client.
pub const DEFAULT_STREAM_BUFFER_SIZE: usize = 8;

/// Clock implementation used by all Snowflake generators.
///
/// This controls how timestamps are embedded into generated IDs.
pub type ClockType = MonotonicClock;

/// Default Snowflake generator used per worker task.
///
/// Each instance is parameterized with a unique machine ID and shared clock.
pub type SnowflakeGeneratorType = BasicSnowflakeGenerator<SnowflakeIdType, ClockType>;
