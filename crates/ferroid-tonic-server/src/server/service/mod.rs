//! gRPC service implementation and worker coordination logic.
//!
//! This module contains the core logic for handling client-facing gRPC requests
//! and delegating work to background worker tasks. It implements the gRPC
//! service and manages streaming request execution, error handling, and
//! shutdown coordination.
//!
//! ## Structure
//!
//! - [`handler`] - gRPC service entry point (`IdService`).

pub mod handler;
