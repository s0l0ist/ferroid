//! Streaming coordinator for chunked Snowflake ID generation.
//!
//! This module orchestrates the generation of large batches of Snowflake IDs by
//! splitting incoming requests into smaller, fixed-size chunks and distributing
//! them across the worker pool. It is responsible for forwarding those chunks
//! back to the gRPC response stream in a backpressure-aware and
//! cancellation-sensitive manner.
//!
//! ## Responsibilities
//!
//! - Split high-level ID generation requests into [`DEFAULT_IDS_PER_CHUNK`]
//!   units.
//! - Route each chunk to the next available worker via [`WorkerPool`].
//! - Relay completed ID chunks or errors to the gRPC client.
//! - Abort early on client-side cancellation or backpressure-induced failure.
//!
//! This function is invoked per-stream by the gRPC service implementation in
//! [`IdService`](crate::server::service::handler::IdService).

use super::request::WorkRequest;
use crate::{
    idgen::IdUnitResponseChunk,
    server::{config::ServerConfig, service::pool::manager::WorkerPool},
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
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
pub(crate) async fn feed_chunks(
    total_ids: usize,
    worker_pool: Arc<WorkerPool>,
    resp_tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    cancel: Arc<CancellationToken>,
    config: ServerConfig,
) {
    let mut remaining = total_ids;

    while remaining > 0 {
        if cancel.is_cancelled() {
            #[cfg(feature = "tracing")]
            tracing::debug!("feed_chunks cancelled");
            break;
        }

        let chunk_size = remaining.min(config.ids_per_chunk);
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
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Response channel failed to forward chunk: {}", _e);
                        return;
                    }
                }
            }
            Err(e) => {
                #[cfg(feature = "tracing")]
                tracing::warn!("Failed to send work to worker: {:?}", e);
                if let Err(_e) = resp_tx.send(Err(e.into())).await {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Response channel failed to forward error: {}", _e);
                }
                return;
            }
        }
    }
}
