//! # Common Snowflake ID Types and Encoding Constants
//!
//! This module defines shared type aliases and constants for working with
//! Snowflake-style IDs in both server and client contexts. It provides a single
//! point of truth for:
//!
//! - The ID format used throughout the system
//! - The expected serialization layout for network transmission
//!
//! These definitions ensure consistency across all components that generate,
//! encode, or decode Snowflake IDs.
//!
//! ## Type Aliases
//!
//! - [`SnowflakeId`] — The canonical Snowflake ID implementation used
//!   (currently [`SnowflakeTwitterId`]).
//! - [`SnowflakeIdTy`] — The primitive integer type backing the ID (typically
//!   `u64`).
//!
//! ## Constants
//!
//! - [`SNOWFLAKE_ID_SIZE`] — The fixed number of bytes needed to encode one ID
//!   in little-endian format.

use ferroid::{Snowflake, SnowflakeTwitterId};

/// The canonical Snowflake ID implementation used across the system.
///
/// By default, this is set to [`SnowflakeTwitterId`], but it can be swapped for
/// any other `Snowflake`-compatible implementation.
pub type SnowflakeId = SnowflakeTwitterId;

/// The primitive integer type that backs a [`SnowflakeId`] (typically `u64`).
pub type SnowflakeIdTy = <SnowflakeId as Snowflake>::Ty;

/// The number of bytes required to encode a single [`SnowflakeId`] in binary
/// form.
///
/// This is used when allocating chunk buffers and parsing packed ID streams.
pub const SNOWFLAKE_ID_SIZE: usize = std::mem::size_of::<SnowflakeIdTy>();
