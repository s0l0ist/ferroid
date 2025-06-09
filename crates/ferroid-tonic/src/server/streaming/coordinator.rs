use super::request::WorkRequest;
use crate::{
    common::{error::IdServiceError, idgen::IdUnitResponseChunk},
    server::{config::ServerConfig, pool::manager::WorkerPool},
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tonic::Status;

/// Coordinates chunked ID generation and forwards results to the gRPC response
/// stream.
///
/// This function splits a large stream request into fixed-size work units and
/// dispatches them to the [`WorkerPool`] for parallel processing. Each response
/// chunk is sent back to the client through a bounded MPSC channel connected to
/// the gRPC stream. If the client disconnects or backpressure exceeds limits,
/// the stream is cancelled early.
///
/// # Arguments
///
/// - `total_ids`: The total number of Snowflake IDs requested by the client.
/// - `worker_pool`: A shared reference to the active [`WorkerPool`] instance.
/// - `resp_tx`: Bounded MPSC channel to forward result chunks to the client.
/// - `cancel`: Cancellation token triggered if the client disconnects or
///   cancels.
///
/// # Behavior
///
/// - If the request is cancelled mid-stream, the function exits cleanly.
/// - Errors encountered during worker dispatch or chunk sending are forwarded
///   to the client and terminate the stream.
/// - Remaining IDs are decremented per chunk, and progress continues until
///   zero.
pub async fn feed_chunks(
    total_ids: usize,
    worker_pool: Arc<WorkerPool>,
    resp_tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    config: ServerConfig,
) -> crate::common::error::Result<()> {
    let mut remaining = total_ids;

    while remaining > 0 {
        let chunk_size = remaining.min(config.ids_per_chunk);
        remaining -= chunk_size;

        let (chunk_tx, mut chunk_rx) = mpsc::channel(2);

        match worker_pool
            .send_to_next_worker(WorkRequest::Stream {
                chunk_size,
                chunk_tx,
            })
            .await
        {
            Ok(()) => {
                while let Some(msg) = chunk_rx.recv().await {
                    // Send the chunk back to the client. If it fails here, we
                    // should immediately return the error so that we can track
                    // it upstream.
                    if let Err(e) = resp_tx.send(msg).await {
                        return Err(IdServiceError::ChannelError {
                            context: format!("Failed to forward chunk: {}", e),
                        });
                    }
                }
            }
            Err(e) => {
                // On an internal error, we make a best effort to surface to the
                // client. But there's a possibility that the client has also
                // disconnected. Therefore, we return the original error and
                // instead log when we are unable to send the error message to
                // the client.
                if let Err(_e) = resp_tx.send(Err(e.clone().into())).await {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("Failed to forward err: {}", _e)
                }
                return Err(e);
            }
        }
    }

    Ok(())
}
