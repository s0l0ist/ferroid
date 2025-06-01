//! Worker pool abstraction for concurrent ID generation.
//!
//! This module manages a set of asynchronous worker tasks responsible for
//! generating Snowflake IDs in parallel. It provides:
//!
//! - Round-robin load balancing across workers
//! - Backpressure-aware channel routing
//! - Graceful shutdown coordination via cancellation tokens and one-shot
//!   responses
//!
//! ## Submodules
//!
//! - [`worker`] - Defines the worker task loop and request execution.
//! - [`manager`] - Orchestrates the pool, routing, and shutdown logic.
//!
//! The pool is constructed and owned by the gRPC [`IdService`] and is used to
//! fulfill stream requests in a scalable, resilient way.
//!
/// [`IdService`]: [`crate::server::service::handler::IdService`]
pub mod manager;
pub mod worker;
