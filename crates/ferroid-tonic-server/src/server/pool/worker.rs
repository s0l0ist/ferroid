use crate::{
    server::service::config::Generator,
    server::streaming::{processor::handle_stream_request, request::WorkRequest},
};
use tokio::sync::mpsc;

/// Worker task responsible for processing [`WorkRequest`] messages.
///
/// Each worker owns its own [`Generator`] to produce unique Snowflake IDs. The
/// worker listens on an MPSC channel and processes requests until a shutdown
/// signal is received.
///
/// This function is designed to be spawned as a Tokio task and runs in an
/// infinite loop until explicitly shut down.
///
/// # Arguments
///
/// - `worker_id`: Unique numeric identifier for this worker (used for
///   logs/tracing).
/// - `rx`: Receiver through which [`WorkRequest`]s are received.
/// - `generator`: A [`Generator`] instance that produces unique IDs for this
///   worker.
/// - `chunk_bytes`: The maximum size (in bytes) for a generated chunk.
///
/// # Request Types
///
/// - [`WorkRequest::Stream`] — Triggers a chunked ID generation request via
///   [`handle_stream_request`].
/// - [`WorkRequest::Shutdown`] — Signals the worker to stop and acknowledge
///   shutdown.
pub async fn worker_loop(
    worker_id: usize,
    mut rx: mpsc::Receiver<WorkRequest>,
    mut generator: Generator,
    chunk_bytes: usize,
) {
    #[cfg(feature = "tracing")]
    tracing::trace!("Worker {worker_id} started");

    // Pre-allocate a reusable buffer to avoid heap churn during ID packing.
    let mut buff = vec![0_u8; chunk_bytes];
    let mut buff_pos = 0;

    while let Some(work) = rx.recv().await {
        match work {
            WorkRequest::Stream {
                chunk_size,
                chunk_tx,
            } => {
                handle_stream_request(
                    worker_id,
                    &mut buff,
                    &mut buff_pos,
                    chunk_bytes,
                    chunk_size,
                    chunk_tx,
                    &mut generator,
                )
                .await;
            }
            WorkRequest::Shutdown { response } => {
                #[cfg(feature = "tracing")]
                tracing::debug!("Worker {worker_id} received shutdown signal");

                if response.send(()).is_err() {
                    #[cfg(feature = "tracing")]
                    tracing::error!("Worker {worker_id} failed to acknowledge shutdown");
                }
                break;
            }
        }
    }

    #[cfg(feature = "tracing")]
    tracing::trace!("Worker {worker_id} stopped");
}
