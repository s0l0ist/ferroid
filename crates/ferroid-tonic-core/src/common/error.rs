//! Error types for the ID generation service.
//!
//! This module defines the central `Error` enum, which captures all recoverable
//! and reportable error cases within the ID generation system. It implements
//! `From<Error>` for `tonic::Status` to enable seamless gRPC error propagation
//! to clients with appropriate status codes and messages.
//!
//! ## Error Cases
//! - `ChannelError`: An internal communication failure between tasks or
//!   workers.
//! - `IdGeneration`: An error occurred during ID creation (via the `ferroid`
//!   generator).
//! - `RequestCancelled`: The client canceled the request mid-flight.
//! - `InvalidRequest`: The client request was malformed or exceeded bounds.
//! - `ServiceShutdown`: A request arrived while the service was shutting down.

use tonic::Status;

pub type Result<T> = core::result::Result<T, Error>;

/// Unified error type for the ID generation service.
#[derive(Clone, thiserror::Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// Internal channel send/receive failure (e.g., closed or full channel).
    #[error("Channel error: {context}")]
    ChannelError { context: String },

    /// Underlying Snowflake ID generation failed. This is currently infallible,
    /// but kept as a placeholder if the generator type yields an error.
    #[error("ID error: {0:?}")]
    IdGeneration(#[from] core::convert::Infallible),

    /// The client aborted the request.
    #[error("Request cancelled by client")]
    RequestCancelled,

    /// The client request was invalid or exceeded constraints.
    #[error("Invalid request: {reason}")]
    InvalidRequest { reason: String },

    /// The service is in the process of shutting down.
    #[error("Service is shutting down")]
    ServiceShutdown,
}

impl From<Error> for Status {
    fn from(err: Error) -> Self {
        match err {
            Error::ChannelError { context } => Self::internal(format!("Channel error: {context}")),
            Error::IdGeneration(e) => Self::internal(format!("ID generation error: {e:?}")),
            Error::RequestCancelled => Self::cancelled("Request was cancelled"),
            Error::InvalidRequest { reason } => Self::invalid_argument(reason),
            Error::ServiceShutdown => Self::unavailable("Service is shutting down"),
        }
    }
}
