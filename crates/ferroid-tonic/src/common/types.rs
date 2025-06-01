//! Common type definitions and constants shared across server and client.
//!
//! This module centralizes shared definitions for the Snowflake ID type and its
//! binary serialization characteristics. It ensures consistent interpretation
//! and encoding of Snowflake IDs across all components, including the gRPC
//! service, client, and serialization logic.
//!
//! ## Type Aliases
//! - [`SnowflakeIdType`]: The concrete Snowflake ID implementation
//!       (Twitter-style).
//! - [`SnowflakeIdTy`]: The underlying primitive integer type used by the
//!       Snowflake implementation.
//!
//! ## Constants
//! - [`SNOWFLAKE_ID_SIZE`]: The number of bytes required to encode a single
//!       Snowflake ID.

use ferroid::{Snowflake, SnowflakeTwitterId};
use std::mem::size_of;

/// The concrete Snowflake ID type used throughout the system.
///
/// Currently set to `SnowflakeTwitterId`, which follows Twitter's original
/// Snowflake format (41-bit timestamp, 10-bit worker, etc.).
pub type SnowflakeIdType = SnowflakeTwitterId;

/// The primitive integer representation of the Snowflake ID (usually `u64`).
pub type SnowflakeIdTy = <SnowflakeIdType as Snowflake>::Ty;

/// Size in bytes of a single serialized Snowflake ID.
///
/// This value is used to allocate buffers for chunked transmission and storage.
pub const SNOWFLAKE_ID_SIZE: usize = size_of::<SnowflakeIdTy>();
