//! Internal message types used for coordinating work across worker tasks.
//!
//! This module defines [`WorkRequest`], the internal command protocol used by
//! the [`WorkerPool`](crate::server::service::pool::manager::WorkerPool) to
//! dispatch work to individual worker tasks. It includes variants for streamed
//! ID generation and graceful shutdown.
//!
//! ## Variants
//!
//! - [`Stream`]: Request to generate a specified number of Snowflake IDs,
//!   streamed back via a channel in chunked form.
//! - [`Shutdown`]: Request to gracefully shut down a worker and acknowledge
//!   termination via a one-shot channel.
//!
//! These messages are sent over bounded MPSC channels from the dispatcher to
//! the worker loop. Cancellation and backpressure logic are handled by the
//! receiver side in the worker implementation.

use crate::idgen::IdUnitResponseChunk;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tonic::Status;

/// A message sent from the worker pool to an individual worker task.
///
/// This enum defines the contract for inter-task communication, enabling the
/// dispatcher to initiate either a chunked ID generation operation or a
/// cooperative shutdown.
///
/// [`WorkRequest`]s are sent over bounded asynchronous channels and are
/// consumed by the worker’s main event loop.
#[derive(Debug)]
pub(crate) enum WorkRequest {
    /// Generate a stream of `count` Snowflake IDs and send them in chunks.
    ///
    /// - `count`: Total number of IDs to generate.
    /// - `tx`: Output channel for sending chunks back to the client stream.
    /// - `cancelled`: Token that allows early exit if the client disconnects.
    Stream {
        count: usize,
        tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
        cancelled: Arc<CancellationToken>,
    },

    /// Request the worker to shut down gracefully.
    ///
    /// - `response`: One-shot channel for acknowledging that the worker has
    ///   completed its shutdown routine.
    Shutdown { response: oneshot::Sender<()> },
}
