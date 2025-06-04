use crate::{
    common::{error::IdServiceError, idgen::IdUnitResponseChunk, types::SNOWFLAKE_ID_SIZE},
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
/// - `_worker_id`: Used for tracing and debugging.
/// - `chunk_size`: Number of Snowflake IDs to generate.
/// - `chunk_tx`: Output channel for sending serialized chunks to the response
///   stream.
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
    chunk_size: usize,
    chunk_tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    cancelled: Arc<CancellationToken>,
    generator: &mut SnowflakeGeneratorType,
    config: &ServerConfig,
) {
    let mut chunk_buf = vec![0_u8; config.chunk_bytes];
    let mut buf_pos = 0;
    let mut generated = 0;

    while generated < chunk_size {
        match generator.try_next_id() {
            Ok(IdGenStatus::Ready { id }) => {
                generated += 1;

                let id_bytes = id.to_raw().to_le_bytes();
                chunk_buf[buf_pos..buf_pos + SNOWFLAKE_ID_SIZE].copy_from_slice(&id_bytes);
                buf_pos += SNOWFLAKE_ID_SIZE;

                if buf_pos == config.chunk_bytes {
                    if should_stop(&chunk_tx, &cancelled) {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Worker {} stopping before chunk send", _worker_id);
                        return;
                    }

                    let bytes = bytes::Bytes::copy_from_slice(&chunk_buf);
                    if let Err(_e) = chunk_tx
                        .send(Ok(IdUnitResponseChunk { packed_ids: bytes }))
                        .await
                    {
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
                if should_stop(&chunk_tx, &cancelled) {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} stopping after ID generation error", _worker_id);
                    return;
                }

                if let Err(_e) = chunk_tx
                    .send(Err(IdServiceError::IdGeneration(e).into()))
                    .await
                {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} failed to send error: {}", _worker_id, _e);
                }

                return;
            }
        }
    }

    // Send final partial chunk if buffer is non-empty
    if buf_pos > 0 && !should_stop(&chunk_tx, &cancelled) {
        let bytes = bytes::Bytes::copy_from_slice(&chunk_buf[..buf_pos]);
        if let Err(_e) = chunk_tx
            .send(Ok(IdUnitResponseChunk { packed_ids: bytes }))
            .await
        {
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
    chunk_tx: &mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    cancelled: &CancellationToken,
) -> bool {
    chunk_tx.is_closed() || cancelled.is_cancelled()
}
