//! Worker lifecycle and stream request processing for Snowflake ID generation.
//!
//! This module implements:
//! - A `WorkRequest` enum to represent stream and shutdown requests sent to
//!   workers.
//! - The main worker loop (`worker_loop`) for handling generation and
//!   cancellation.
//! - Streamed ID generation logic with chunk buffering
//!   (`handle_stream_request`).
//!
//! Each worker is a long-running async task responsible for producing Snowflake
//! IDs in response to chunked stream requests. Workers can be shut down via a
//! signal, allowing for coordinated service shutdowns.

use super::request::WorkRequest;
use crate::{
    common::SNOWFLAKE_ID_SIZE,
    config::{DEFAULT_CHUNK_BYTES, SnowflakeGeneratorType},
    error::IdServiceError,
    idgen::IdUnitResponseChunk,
};
use ferroid::{IdGenStatus, Snowflake};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tonic::Status;

/// Asynchronous task representing a single worker's lifecycle.
///
/// Each worker listens for `WorkRequest`s on a channel, processes ID generation
/// requests, and exits cleanly when either:
/// - Its input channel is closed,
/// - It receives a shutdown request,
/// - The global shutdown token is cancelled.
///
/// # Arguments
/// - `worker_id`: Identifier used for logging and shard allocation.
/// - `rx`: Channel receiver for `WorkRequest`s.
/// - `generator`: Snowflake generator instance scoped to this worker.
/// - `shutdown_token`: Cancellation token used to trigger service-wide
///   shutdown.
pub(crate) async fn worker_loop(
    worker_id: usize,
    mut rx: mpsc::Receiver<WorkRequest>,
    mut generator: SnowflakeGeneratorType,
    shutdown_token: CancellationToken,
) {
    #[cfg(feature = "tracing")]
    tracing::debug!("Worker {} started", worker_id);

    loop {
        tokio::select! {
            work = rx.recv() => {
                match work {
                    Some(WorkRequest::Stream { count, tx, cancelled }) => {
                        handle_stream_request(worker_id, count, tx, cancelled, &mut generator).await;
                    }
                    Some(WorkRequest::Shutdown { response }) => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Worker {} received shutdown signal", worker_id);
                        let _ = response.send(());
                        break;
                    }
                    None => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Worker {} channel closed", worker_id);
                        break;
                    }
                }
            }
            _ = shutdown_token.cancelled() => {
                #[cfg(feature = "tracing")]
                tracing::debug!("Worker {} shutdown via cancellation token", worker_id);
                break;
            }
        }
    }

    #[cfg(feature = "tracing")]
    tracing::debug!("Worker {} stopped", worker_id);
}

/// Handles a single stream generation request.
///
/// Batches the requested count into a fixed-size buffer and emits chunks of
/// serialized Snowflake IDs to the client. Responds early to cancellation or
/// backpressure.
///
/// # Arguments
/// - `worker_id`: Used for logging purposes.
/// - `count`: Total number of IDs to generate.
/// - `tx`: Channel to send chunks back to the client.
/// - `cancelled`: Cancellation token for early termination.
/// - `generator`: The Snowflake ID generator used to produce unique IDs.
async fn handle_stream_request(
    _worker_id: usize,
    count: usize,
    tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    cancelled: Arc<CancellationToken>,
    generator: &mut SnowflakeGeneratorType,
) {
    let mut chunk_buf = [0_u8; DEFAULT_CHUNK_BYTES];
    let mut buf_pos = 0;
    let mut generated = 0;

    while generated < count {
        match generator.try_next_id() {
            Ok(IdGenStatus::Ready { id }) => {
                generated += 1;
                let id_bytes = id.to_raw().to_le_bytes();
                chunk_buf[buf_pos..buf_pos + SNOWFLAKE_ID_SIZE].copy_from_slice(&id_bytes);
                buf_pos += SNOWFLAKE_ID_SIZE;

                if buf_pos == DEFAULT_CHUNK_BYTES {
                    if should_stop(&cancelled, &tx) {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Worker {} stopping before chunk send", _worker_id);
                        return;
                    }

                    let bytes = bytes::Bytes::copy_from_slice(&chunk_buf);
                    if let Err(_e) = tx.send(Ok(IdUnitResponseChunk { packed_ids: bytes })).await {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Worker {} failed to send chunk: {}", _worker_id, _e);
                        return;
                    }
                    buf_pos = 0;
                }
            }
            Ok(IdGenStatus::Pending { .. }) => {
                tokio::task::yield_now().await;
            }
            Err(e) => {
                if should_stop(&cancelled, &tx) {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} stopping after ID generation error", _worker_id);
                    return;
                }

                if let Err(_send_err) = tx.send(Err(IdServiceError::IdGeneration(e).into())).await {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} failed to send error: {}", _worker_id, _send_err);
                }
                return;
            }
        }
    }

    // Send any remaining partial chunk
    if buf_pos > 0 && !should_stop(&cancelled, &tx) {
        let bytes = bytes::Bytes::copy_from_slice(&chunk_buf[..buf_pos]);
        if let Err(_e) = tx.send(Ok(IdUnitResponseChunk { packed_ids: bytes })).await {
            #[cfg(feature = "tracing")]
            tracing::debug!("Worker {} failed to send final chunk: {}", _worker_id, _e);
        }
    }
}

/// Determines whether the current stream request should be aborted.
///
/// A request should terminate early if the downstream response channel is
/// closed or if the cancellation token has been triggered.
fn should_stop(
    cancelled: &CancellationToken,
    tx: &mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
) -> bool {
    tx.is_closed() || cancelled.is_cancelled()
}
