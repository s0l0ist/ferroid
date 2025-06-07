//! Server-side components of the `ferroid-tonic` ID generation service.
//!
//! This module contains the building blocks necessary to run the streaming gRPC
//! server, including service logic, worker pool orchestration, and telemetry
//! setup.
//!
//! ## Submodules
//!
//! - [`service`] - Core gRPC service implementation, including request
//!   handling, worker coordination, and stream chunking.
//! - [`telemetry`] - Tracing-based structured logging initialization
//!   (optional).
//!
//! These components are wired together in the server's `main.rs` and used to
//! serve the `IdGen` gRPC service defined in [`crate::idgen`].

pub mod config;
pub mod pool;
pub mod service;
pub mod streaming;
pub mod telemetry;
