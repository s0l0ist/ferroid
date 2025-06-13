//! Shared types and error definitions used across the `ferroid-tonic` server.
//!
//! The `common` module provides reusable abstractions shared by the gRPC
//! service layer, worker pool, and stream coordination logic. These types are
//! decoupled from specific layers and are used throughout the `server` for
//! consistent ID encoding and error propagation.
//!
//! ## Submodules
//!
//! - [`Error`]: Unified service error type for consistent error handling.
//! - [`Result`]: Type alias for `Result<T, Error>`.
//! - [`types`]: Constants and ID-related type aliases.
//!
//! These modules are designed for cross-cutting concerns and are imported
//! widely within the server implementation.

mod error;
mod result;
pub mod types;

pub use error::Error;
pub use result::Result;

pub mod ferroid {
    tonic::include_proto!("ferroid");
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("ferroid_descriptor");
}
