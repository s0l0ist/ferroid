//! Internal work request types sent to worker tasks.
//!
//! This module defines the [`WorkRequest`] enum used to communicate between the
//! worker pool and individual worker tasks. It includes stream generation and
//! shutdown requests, along with shared cancellation coordination.

use crate::idgen::IdUnitResponseChunk;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tonic::Status;

/// A message sent from the dispatcher to a worker.
///
/// - [`Stream`] is used to request a fixed number of Snowflake IDs.
/// - [`Shutdown`] is used to gracefully terminate the worker.
#[derive(Debug)]
pub(crate) enum WorkRequest {
    /// Generate a stream of `count` Snowflake IDs and send them through `tx`.
    Stream {
        count: usize,
        tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
        cancelled: Arc<CancellationToken>,
    },

    /// Initiate graceful shutdown and notify via the `response` channel.
    Shutdown { response: oneshot::Sender<()> },
}
