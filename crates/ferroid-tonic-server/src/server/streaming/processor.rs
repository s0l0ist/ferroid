use ferroid_tonic_core::{
    ferroid::IdGenStatus,
    proto::IdChunk,
    types::{Generator, SNOWFLAKE_ID_SIZE},
    Error,
};
use tokio::sync::mpsc;
use tonic::Status;

/// Handles a single ID generation task within a worker.
///
/// This function generates a fixed number of Snowflake-style IDs (`chunk_size`)
/// using the worker's [`Generator`]. IDs are written into a reusable byte
/// buffer and streamed back to the client as serialized [`IdChunk`] messages.
///
/// Chunks are emitted once the buffer is full, and any remaining IDs are
/// flushed at the end as a partial chunk. If the client disconnects or the
/// output channel is closed, the task exits early.
///
/// # Arguments
///
/// - `_worker_id`: Identifier for this worker, used in logs and tracing.
/// - `chunk_buf`: Preallocated buffer to hold packed ID bytes.
/// - `buff_pos`: Tracks the write position within `chunk_buf`.
/// - `chunk_bytes`: Maximum number of bytes per response chunk.
/// - `chunk_size`: Total number of IDs to generate.
/// - `chunk_tx`: Channel used to send completed chunks back to the gRPC stream.
/// - `generator`: Local Snowflake ID generator owned by the worker.
///
/// # Behavior
///
/// - Generates exactly `chunk_size` IDs unless cancelled early.
/// - Batches IDs into fixed-size chunks and sends them through `chunk_tx`.
/// - On error, sends a single [`Error::IdGeneration`] response and exits.
/// - Uses cooperative yielding (`yield_now`) when generation is pending.
#[allow(clippy::needless_pass_by_ref_mut)]
#[allow(clippy::used_underscore_binding)]
pub async fn handle_stream_request(
    _worker_id: usize,
    chunk_buf: &mut [u8],
    buff_pos: &mut usize,
    chunk_bytes: usize,
    chunk_size: usize,
    chunk_tx: mpsc::Sender<Result<IdChunk, Status>>,
    // Use `&mut` so this `Send` future doesn't require `Generator: Sync`.
    generator: &mut Generator,
) {
    let mut generated = 0;

    while generated < chunk_size {
        match generator.try_next_id() {
            Ok(IdGenStatus::Ready { id }) => {
                generated += 1;

                // Write the ID as little-endian bytes into the buffer.
                let id_bytes = id.to_raw().to_le_bytes();
                chunk_buf[*buff_pos..*buff_pos + SNOWFLAKE_ID_SIZE].copy_from_slice(&id_bytes);
                *buff_pos += SNOWFLAKE_ID_SIZE;

                // If the buffer is full, send it as a chunk.
                if *buff_pos == chunk_bytes {
                    if chunk_tx.is_closed() {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Worker {_worker_id} exiting before chunk send");
                        return;
                    }

                    let bytes = bytes::Bytes::copy_from_slice(chunk_buf);
                    if let Err(_e) = chunk_tx.send(Ok(IdChunk { packed_ids: bytes })).await {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Worker {_worker_id} failed to send chunk: {_e}");
                        return;
                    }

                    *buff_pos = 0;
                }
            }
            Ok(IdGenStatus::Pending { .. }) => {
                // Yield to the scheduler to avoid busy looping.
                tokio::task::yield_now().await;
            }
            Err(e) => {
                if chunk_tx.is_closed() {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {_worker_id} exiting after generation error");
                    return;
                }

                if let Err(_e) = chunk_tx.send(Err(Error::IdGeneration(e).into())).await {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {_worker_id} failed to send error: {_e}");
                }

                return;
            }
        }
    }

    // Flush final partial chunk if anything remains in the buffer.
    if *buff_pos > 0 && !chunk_tx.is_closed() {
        let bytes = bytes::Bytes::copy_from_slice(&chunk_buf[..*buff_pos]);
        *buff_pos = 0;
        if let Err(_e) = chunk_tx.send(Ok(IdChunk { packed_ids: bytes })).await {
            #[cfg(feature = "tracing")]
            tracing::debug!("Worker {_worker_id} failed to send final chunk: {_e}");
        }
    }
}
