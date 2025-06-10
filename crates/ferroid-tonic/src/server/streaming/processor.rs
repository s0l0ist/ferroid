use crate::service::config::Generator;
use ferroid::{IdGenStatus, Snowflake};
use ferroid_tonic::common::{Error, idgen::IdUnitResponseChunk, types::SNOWFLAKE_ID_SIZE};
use tokio::sync::mpsc;
use tonic::Status;

/// Handles a single streamed ID generation task for a worker.
///
/// IDs are generated sequentially using the provided [`Generator`]
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
/// - Propagates [`Error::IdGeneration`] to the client on failure.
pub async fn handle_stream_request(
    _worker_id: usize,
    chunk_buf: &mut [u8],
    buff_pos: &mut usize,
    chunk_bytes: usize,
    chunk_size: usize,
    chunk_tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    generator: &mut Generator,
) {
    let mut generated = 0;

    while generated < chunk_size {
        match generator.try_next_id() {
            Ok(IdGenStatus::Ready { id }) => {
                generated += 1;

                let id_bytes = id.to_raw().to_le_bytes();
                chunk_buf[*buff_pos..*buff_pos + SNOWFLAKE_ID_SIZE].copy_from_slice(&id_bytes);
                *buff_pos += SNOWFLAKE_ID_SIZE;

                if *buff_pos == chunk_bytes {
                    if chunk_tx.is_closed() {
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

                    *buff_pos = 0;
                }
            }
            Ok(IdGenStatus::Pending { .. }) => {
                // Tokio's timer granularity is 1ms, but `yield_now` requeues
                // the task allowing others to proceed. By the time we're polled
                // again, enough time has typically passed for progress to
                // resume.
                tokio::task::yield_now().await;
            }
            Err(e) => {
                if chunk_tx.is_closed() {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} stopping after ID generation error", _worker_id);
                    return;
                }

                if let Err(_e) = chunk_tx.send(Err(Error::IdGeneration(e).into())).await {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} failed to send error: {}", _worker_id, _e);
                }

                return;
            }
        }
    }

    // Send final partial chunk if buffer is non-empty
    if *buff_pos > 0 && !chunk_tx.is_closed() {
        let bytes = bytes::Bytes::copy_from_slice(&chunk_buf[..*buff_pos]);
        *buff_pos = 0;
        if let Err(_e) = chunk_tx
            .send(Ok(IdUnitResponseChunk { packed_ids: bytes }))
            .await
        {
            #[cfg(feature = "tracing")]
            tracing::debug!("Worker {} failed to send final chunk: {}", _worker_id, _e);
        }
    }
}
