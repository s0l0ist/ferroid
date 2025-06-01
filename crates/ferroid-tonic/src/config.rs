//! Configuration constants and type aliases for the ID generation server.
//!
//! This module defines system-wide constants and type aliases for controlling
//! concurrency, memory usage, and Snowflake ID generation in the service. These
//! values affect how the worker pool is sized, how data is chunked, and how
//! communication channels are buffered.
//!
//! ## Key Concepts
//! - **Sharding**: Each worker is assigned a unique machine ID to prevent
//!   Snowflake ID collisions.
//! - **Backpressure**: Buffer sizes are tuned to balance throughput and memory
//!   usage.
//! - **Chunking**: IDs are grouped into fixed-size byte chunks to optimize
//!   network transfer.
//!
//! ## Type Aliases
//! - [`ClockType`] defines the timestamp source used by each generator.
//! - [`SnowflakeGeneratorType`] is the concrete generator implementation used
//!   by workers.
//!
//! ## Compile-time Safety
//! This module performs a compile-time assertion to ensure that the number of
//! workers does not exceed the available Snowflake machine ID space, which
//! would otherwise introduce ID collisions.

use crate::common::{SNOWFLAKE_ID_SIZE, SnowflakeIdType};
use ferroid::{BasicSnowflakeGenerator, MonotonicClock};

/// Offset used when assigning Snowflake worker IDs. Used to avoid overlap if
/// multiple clusters share a global ID space.
pub const SHARD_OFFSET: usize = 0;

/// Number of concurrent ID generation workers (each owns a unique generator).
pub const NUM_WORKERS: usize = 512;

/// Upper bound on the number of IDs that can be requested in a single stream
/// request.
pub const MAX_ALLOWED_IDS: usize = 1_000_000_000;

/// Size of the channel buffer used to queue work requests per worker.
pub const DEFAULT_WORK_REQUEST_BUFFER_SIZE: usize = 2048;

/// Number of IDs packed into each response chunk. Affects batch size and memory
/// usage.
pub const DEFAULT_IDS_PER_CHUNK: usize = 2048;

/// Total number of bytes per ID chunk. Derived from `DEFAULT_IDS_PER_CHUNK *
/// SNOWFLAKE_ID_SIZE`.
pub const DEFAULT_CHUNK_BYTES: usize = DEFAULT_IDS_PER_CHUNK * SNOWFLAKE_ID_SIZE;

/// Number of buffered chunks allowed in the gRPC streaming response. Larger
/// values enable more aggressive pipelining.
pub const DEFAULT_STREAM_BUFFER_SIZE: usize = 8;

/// Clock source used for Snowflake ID generation.
pub type ClockType = MonotonicClock;

/// Default Snowflake generator type used by each worker.
pub type SnowflakeGeneratorType = BasicSnowflakeGenerator<SnowflakeIdType, ClockType>;

/// Ensure the number of workers does not exceed the representable machine ID
/// space. This protects against ID collisions at compile time.
const _: () = assert!(NUM_WORKERS <= (SnowflakeIdType::max_machine_id() as usize) + 1);
