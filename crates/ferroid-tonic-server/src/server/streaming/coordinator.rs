use super::request::WorkRequest;
use crate::server::{config::ServerConfig, pool::manager::WorkerPool};
use ferroid_tonic_core::{Error, proto::IdChunk};
use std::sync::Arc;
use tokio::sync::mpsc;
use tonic::Status;

/// Splits a large ID generation request into chunks and delegates them to
/// workers.
///
/// This function handles the coordination logic for streaming Snowflake IDs
/// back to the client. It breaks the total request into fixed-size chunks,
/// dispatches each chunk to a worker via the [`WorkerPool`], and forwards
/// results to the response stream channel.
///
/// If any error occurs during dispatch or transmission, the stream is
/// terminated early and the error is surfaced to the client.
///
/// # Arguments
///
/// - `total_ids`: Total number of IDs requested by the client.
/// - `worker_pool`: Shared pool of background workers responsible for
///   generation.
/// - `resp_tx`: Channel used to stream response chunks back to the client.
/// - `config`: Server configuration, including chunk size and limits.
///
/// # Behavior
///
/// - Uses a per-chunk buffer size defined by `ids_per_chunk`.
/// - Sends generated `IdChunk`s back to the gRPC stream channel (`resp_tx`).
/// - On dispatch or send error, returns early and attempts to surface the
///   issue.
/// - Gracefully exits if the client disconnects mid-stream.
pub async fn feed_chunks(
    total_ids: usize,
    worker_pool: Arc<WorkerPool>,
    resp_tx: mpsc::Sender<Result<IdChunk, Status>>,
    config: ServerConfig,
) -> ferroid_tonic_core::Result<()> {
    let mut remaining = total_ids;

    while remaining > 0 {
        let chunk_size = remaining.min(config.ids_per_chunk);
        remaining -= chunk_size;

        // Temporary channel used to receive the generated chunk from the
        // worker.
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
                        return Err(Error::ChannelError {
                            context: format!("Failed to forward chunk: {e}"),
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
                    tracing::warn!("Failed to forward err: {}", _e);
                }
                return Err(e);
            }
        }
    }

    Ok(())
}
