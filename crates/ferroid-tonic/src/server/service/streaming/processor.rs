//! Streamed Snowflake ID generation with chunk buffering and cancellation
//! support.
//!
//! This module defines the lower-level execution logic for generating Snowflake
//! IDs in response to a `WorkRequest::Stream` command. It handles:
//!
//! - ID generation via a worker-local [`SnowflakeGeneratorType`].
//! - Efficient buffering of Snowflake IDs into pre-sized byte chunks.
//! - Early exit on cancellation or gRPC backpressure.
//!
//! Each worker task invokes [`handle_stream_request`] to fulfill part of a
//! larger client stream request, emitting serialized chunks of Snowflake IDs
//! back to the gRPC response pipeline.
//!
//! ## Responsibilities
//!
//! - Use [`try_next_id`] from `ferroid` to fetch unique Snowflake IDs.
//! - Pack IDs into contiguous byte buffers sized by [`DEFAULT_CHUNK_BYTES`].
//! - Transmit each chunk through the provided MPSC channel.
//! - Respect cancellation tokens and channel closures to avoid wasted work.

use crate::{
    common::{error::IdServiceError, types::SNOWFLAKE_ID_SIZE},
    idgen::IdUnitResponseChunk,
    server::{config::ServerConfig, service::config::SnowflakeGeneratorType},
};
use ferroid::{IdGenStatus, Snowflake};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tonic::Status;

/// Handles a single streamed ID generation task for a worker.
///
/// IDs are generated sequentially using the provided [`SnowflakeGeneratorType`]
/// and buffered into a fixed-size byte array. When the buffer is full, it is
/// sent downstream as a gRPC response chunk. Remaining IDs (if any) are sent as
/// a final partial chunk.
///
/// The function respects both explicit client-side cancellation and
/// backpressure (closed channels) by checking cancellation before sending each
/// chunk.
///
/// # Arguments
///
/// - `worker_id`: Used for tracing and debugging.
/// - `count`: Number of Snowflake IDs to generate in total.
/// - `tx`: Output channel for sending serialized chunks to the response stream.
/// - `cancelled`: Shared cancellation token (triggered by stream termination).
/// - `generator`: Local Snowflake ID generator unique to the worker.
///
/// # Behavior
///
/// - Aborts early if the cancellation token is triggered or the output channel
///   is closed.
/// - Emits zero or more [`IdUnitResponseChunk`]s, depending on how many IDs
///   were successfully generated.
/// - Propagates [`IdServiceError::IdGeneration`] to the client on failure.
pub async fn handle_stream_request(
    _worker_id: usize,
    count: usize,
    tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    cancelled: Arc<CancellationToken>,
    generator: &mut SnowflakeGeneratorType,
    config: &ServerConfig,
) {
    let mut chunk_buf = vec![0_u8; config.chunk_bytes];
    let mut buf_pos = 0;
    let mut generated = 0;

    while generated < count {
        match generator.try_next_id() {
            Ok(IdGenStatus::Ready { id }) => {
                generated += 1;

                let id_bytes = id.to_raw().to_le_bytes();
                chunk_buf[buf_pos..buf_pos + SNOWFLAKE_ID_SIZE].copy_from_slice(&id_bytes);
                buf_pos += SNOWFLAKE_ID_SIZE;

                if buf_pos == config.chunk_bytes {
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

                if let Err(_e) = tx.send(Err(IdServiceError::IdGeneration(e).into())).await {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} failed to send error: {}", _worker_id, _e);
                }

                return;
            }
        }
    }

    // Send final partial chunk if buffer is non-empty
    if buf_pos > 0 && !should_stop(&cancelled, &tx) {
        let bytes = bytes::Bytes::copy_from_slice(&chunk_buf[..buf_pos]);
        if let Err(_e) = tx.send(Ok(IdUnitResponseChunk { packed_ids: bytes })).await {
            #[cfg(feature = "tracing")]
            tracing::debug!("Worker {} failed to send final chunk: {}", _worker_id, _e);
        }
    }
}

/// Determines whether the stream should terminate early.
///
/// Returns `true` if the client has cancelled the request or if the response
/// channel is already closed.
fn should_stop(
    cancelled: &CancellationToken,
    tx: &mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
) -> bool {
    tx.is_closed() || cancelled.is_cancelled()
}
