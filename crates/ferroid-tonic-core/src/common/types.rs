//! # Common Snowflake ID Types and Constants
//!
//! This module defines the shared types and constants used for generating,
//! encoding, and decoding Snowflake-style IDs across the system. It ensures
//! that client and server components adhere to a consistent, compile-time
//! contract for binary serialization.
//!
//! ## Overview
//!
//! - Defines the canonical Snowflake ID layout used by default
//! - Provides core type aliases and timestamp logic
//! - Enables extensibility for custom ID formats
//!
//! ## Type Aliases
//!
//! - [`SnowflakeId`] - The default Snowflake ID type (backed by
//!   [`SnowflakeTwitterId`])
//! - [`SnowflakeIdTy`] - The primitive integer backing the ID (typically `u64`)
//! - [`Clock`] - The system clock used for timestamp embedding
//! - [`Generator`] - The default ID generator used by worker tasks
//!
//! ## Constants
//!
//! - [`SNOWFLAKE_ID_SIZE`] - Size (in bytes) of a serialized ID
//! - [`EPOCH`] - Epoch offset used for timestamp generation
//!
//! ## Customization
//!
//! Advanced users can define their own Snowflake ID layout using the
//! [`define_snowflake_id!`](ferroid::define_snowflake_id) macro from the
//! [`ferroid`] crate. This allows full control over timestamp width, machine ID
//! bits, sequence size, and more.
//!
//! To use a custom ID format:
//!
//! 1. Create a new binary or library crate
//! 2. Invoke `define_snowflake_id!` with your desired layout
//! 3. Replace [`SnowflakeId`] with your custom type throughout your app
//!
//! > ⚠️ The default [`SnowflakeId`] is fixed at compile time and not
//! > dynamically swappable. This enforces a strict client-server contract for
//! > binary encoding. To change the format, fork or wrap the binary crate and
//! > inject your own implementation.

use ferroid::{BasicSnowflakeGenerator, Id, MonotonicClock, SnowflakeTwitterId, TWITTER_EPOCH};

/// The canonical Snowflake ID type used across the system.
///
/// Defaults to [`SnowflakeTwitterId`], but can be replaced in custom builds
/// with any type implementing [`SnowflakeId`].
///
/// [`SnowflakeId`]: crate::ferroid::SnowflakeId
pub type SnowflakeId = SnowflakeTwitterId;

/// The primitive integer type that backs a [`SnowflakeId`] (typically `u64`).
pub type SnowflakeIdTy = <SnowflakeId as Id>::Ty;

/// The number of bytes required to serialize a single [`SnowflakeId`] in
/// little-endian format.
///
/// Used for allocating buffers and decoding packed ID streams.
pub const SNOWFLAKE_ID_SIZE: usize = core::mem::size_of::<SnowflakeIdTy>();

/// The system clock used by ID generators for timestamp encoding.
pub type Clock = MonotonicClock;

/// The epoch offset used as the zero-point for timestamp calculations.
///
/// This value is subtracted from the system clock to produce smaller, monotonic
/// timestamps. Defaults to [`TWITTER_EPOCH`].
pub const EPOCH: core::time::Duration = TWITTER_EPOCH;

/// The default Snowflake ID generator used by each worker.
///
/// Parameterized with a unique machine ID and a shared [`Clock`].
pub type Generator = BasicSnowflakeGenerator<SnowflakeId, Clock>;
