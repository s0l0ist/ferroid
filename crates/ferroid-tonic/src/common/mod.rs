//! Shared types and error definitions used across the `ferroid-tonic` server.
//!
//! The `common` module defines reusable abstractions that are shared across the
//! gRPC service layer, worker pool, and stream coordination logic.
//!
//! ## Submodules
//!
//! - [`error`] - Centralized service error type used throughout request
//!   handling.
//! - [`types`] - Common constants and ID-related type aliases.
//!
//! These definitions are not tied to any specific layer and are imported
//! throughout the `server` module for error propagation and ID encoding.

pub mod error;
pub mod types;
