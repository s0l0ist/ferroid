//! gRPC service implementation and worker coordination logic.
//!
//! This module contains the core logic for handling client-facing gRPC requests
//! and delegating work to background worker tasks. It implements the [`IdGen`]
//! gRPC service defined in [`crate::idgen`] and manages streaming request
//! execution, error handling, and shutdown coordination.
//!
//! ## Structure
//!
//! - [`config`] - Compile-time tuning parameters and generator type aliases.
//! - [`handler`] - gRPC service entry point (`IdService`).
//! - [`pool`] - Load-balanced, backpressure-aware worker pool.
//! - [`streaming`] - Chunked stream coordination logic.
//!
//! This module is publicly re-exported at the crate root to simplify usage from
//! `main.rs`.
//!
//! [`IdGen`]: [`crate::idgen::id_gen_server::IdGen`]
//! [`IdService`]: [`crate::server::service::handler::IdService`]

pub mod config;
pub mod handler;
mod pool;
mod streaming;
