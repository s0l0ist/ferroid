//! Stream forwarding and chunk orchestration for Snowflake ID generation.
//!
//! This module defines the logic that coordinates chunked ID generation across
//! multiple workers and forwards the resulting data back to the gRPC response
//! stream.
//!
//! ## Responsibilities
//! - Split incoming requests into fixed-size chunks.
//! - Send `WorkRequest::Stream` messages to the worker pool.
//! - Forward responses (success or error) to the client.
//! - Abort cleanly on cancellation or backpressure failures.

use super::{pool::WorkerPool, request::WorkRequest};
use crate::{idgen::IdUnitResponseChunk, server::config::DEFAULT_IDS_PER_CHUNK};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tonic::Status;

/// Orchestrates chunked ID generation and forwards results to the response
/// stream.
///
/// This function is invoked per gRPC stream request. It:
/// - Splits the requested number of IDs into [`DEFAULT_IDS_PER_CHUNK`]-sized
///   chunks.
/// - Sends each chunk request to the next available worker via [`WorkerPool`].
/// - Forwards all received chunks to the client through `resp_tx`.
/// - Terminates early if the request is cancelled or a communication error
///   occurs.
///
/// # Arguments
/// - `total_ids`: Total number of IDs requested by the client.
/// - `worker_pool`: Shared reference to the worker pool to distribute work.
/// - `resp_tx`: Sender side of the channel connected to the gRPC response
///   stream.
/// - `cancel`: Cancellation token used to abort if the client disconnects or
///   cancels.
///
/// # Cancellation
/// The task exits cleanly on `cancel.is_cancelled()`, avoiding unnecessary
/// work.
pub(crate) async fn feed_chunks(
    total_ids: usize,
    worker_pool: Arc<WorkerPool>,
    resp_tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    cancel: Arc<CancellationToken>,
) {
    let mut remaining = total_ids;

    while remaining > 0 {
        // If the client canceled (disconnect), stop producing work
        if cancel.is_cancelled() {
            #[cfg(feature = "tracing")]
            tracing::debug!("feed_chunks cancelled");
            break;
        }

        let chunk_size = remaining.min(DEFAULT_IDS_PER_CHUNK);
        remaining -= chunk_size;

        let (chunk_tx, mut chunk_rx) = mpsc::channel(2);

        match worker_pool
            .send_to_next_worker(WorkRequest::Stream {
                count: chunk_size,
                tx: chunk_tx,
                cancelled: cancel.clone(),
            })
            .await
        {
            Ok(()) => {
                while let Some(msg) = chunk_rx.recv().await {
                    if let Err(_e) = resp_tx.send(msg).await {
                        // typically "channel closed" (client disconnect)
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Response channel failed to forward chunk: {}", _e);
                        return;
                    }
                }
            }
            Err(e) => {
                // typically "ServerShutdown" (server disconnect)
                #[cfg(feature = "tracing")]
                tracing::warn!("Failed to send work to worker: {:?}", e);
                if let Err(_e) = resp_tx.send(Err(e.into())).await {
                    // typically "channel closed" (client disconnect)
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Response channel failed to forward error: {}", _e);
                }
                return;
            }
        }
    }
}
