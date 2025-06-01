//! Error types for the ID generation service.
//!
//! This module defines the central `IdServiceError` enum, which captures all
//! recoverable and reportable error cases within the ID generation system. It
//! implements `From<IdServiceError>` for `tonic::Status` to enable seamless
//! gRPC error propagation to clients with appropriate status codes and
//! messages.
//!
//! ## Error Cases
//! - `ChannelError`: An internal communication failure between tasks or
//!   workers.
//! - `IdGeneration`: An error occurred during ID creation (via the `ferroid`
//!   generator).
//! - `ServiceOverloaded`: Backpressure or queue limits were exceeded.
//! - `RequestCancelled`: The client canceled the request mid-flight.
//! - `InvalidRequest`: The client request was malformed or exceeded bounds.
//! - `ServiceShutdown`: A request arrived while the service was shutting down.

use thiserror::Error;
use tonic::Status;

/// Unified error type for the ID generation service.
#[derive(Error, Debug)]
pub enum IdServiceError {
    /// Internal channel send/receive failure (e.g., closed or full channel).
    #[error("Channel communication error: {context}")]
    ChannelError { context: String },

    /// Underlying Snowflake ID generation failed.
    #[error("ID generation failed: {0:?}")]
    IdGeneration(#[from] ferroid::Error),

    /// The service cannot handle more requests due to load.
    #[error("Service is overloaded: {details}")]
    ServiceOverloaded { details: String },

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

impl From<IdServiceError> for Status {
    fn from(err: IdServiceError) -> Self {
        match err {
            IdServiceError::ChannelError { context } => {
                Status::unavailable(format!("Channel error: {}", context))
            }
            IdServiceError::IdGeneration(e) => {
                Status::internal(format!("ID generation error: {:?}", e))
            }
            IdServiceError::ServiceOverloaded { details } => Status::resource_exhausted(details),
            IdServiceError::RequestCancelled => Status::cancelled("Request was cancelled"),
            IdServiceError::InvalidRequest { reason } => Status::invalid_argument(reason),
            IdServiceError::ServiceShutdown => Status::unavailable("Service is shutting down"),
        }
    }
}
