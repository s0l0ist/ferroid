use ferroid_tonic::common::ferroid::IdChunk;
use tokio::sync::{mpsc, oneshot};
use tonic::Status;

/// A message sent from the dispatcher to a worker task.
///
/// This enum represents all possible commands a worker can receive, either to
/// process a stream of IDs or to initiate a cooperative shutdown.
///
/// [`WorkRequest`] values are sent over bounded MPSC channels and processed
/// inside each worker's async event loop.
#[derive(Debug)]
pub enum WorkRequest {
    /// Generate and stream a fixed number of Snowflake IDs.
    ///
    /// Each worker receiving this variant is responsible for generating
    /// `chunk_size` unique IDs and returning them in binary-packed form via the
    /// provided channel.
    ///
    /// - `chunk_size`: Number of IDs to generate in this work unit.
    /// - `chunk_tx`: Channel used to send [`IdChunk`] results to the gRPC
    ///   response stream.
    Stream {
        chunk_size: usize,
        chunk_tx: mpsc::Sender<Result<IdChunk, Status>>,
    },

    /// Gracefully shuts down the worker after completing all in-flight work.
    ///
    /// This variant is sent during service shutdown. The worker is expected to
    /// exit cleanly and signal completion by sending `()` through the provided
    /// one-shot channel.
    ///
    /// - `response`: Channel used to acknowledge the shutdown.
    Shutdown { response: oneshot::Sender<()> },
}
