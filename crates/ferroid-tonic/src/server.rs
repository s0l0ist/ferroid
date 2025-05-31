//! gRPC ID generation service using ferroid and tonic. Provides single, batch,
//! and streaming ID generation via Snowflake-like IDs with cancellation
//! support.

use core::pin::Pin;
use ferroid::{
    BasicSnowflakeGenerator, IdGenStatus, MonotonicClock, Snowflake, SnowflakeTwitterId,
};
use futures::{StreamExt, stream::SelectAll};
use idgen::{
    IdStreamRequest, IdUnitResponseChunk,
    id_gen_server::{IdGen, IdGenServer},
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status, codec::CompressionEncoding, transport::Server};

pub mod idgen {
    tonic::include_proto!("idgen");
}

#[derive(Debug)]
enum WorkRequest {
    Stream {
        count: usize,
        tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
        cancelled: Arc<CancellationToken>,
    },
}

struct IdService {
    workers: Arc<Vec<mpsc::Sender<WorkRequest>>>,
}

/// ID type used by this server instance
type ServerIdType = SnowflakeTwitterId;

/// The offset to shard the generators
const SHARD_OFFSET: usize = 0;
/// Number of worker tasks processing ID generation requests concurrently. Each
/// worker handles one generation request at a time but can process multiple
/// chunks within that request. This
const NUM_WORKERS: usize = 1024;
// Increase this number helps when concurrency is high at the cost of more memory
// for in-flight streams.

/// Buffer capacity for each worker's mpsc channel. Controls how many work
/// requests can be queued per worker before backpressure kicks in. This is
/// measured in requests, not memory usage.
const DEFAULT_WORK_REQUEST_BUFFER_SIZE: usize = 256;

/// Number of Snowflake IDs packed into each response chunk. 4096 IDs per chunk
/// This balances network efficiency with memory pressure.
const DEFAULT_IDS_PER_CHUNK: usize = 1024;
// The higher this number, the fewer dispatched WorkRequest's when the ID count
// is high which saves a ton of memory. The catch, is this wastes memory on low
// ID count requests, but they don't tend to stay in memory very long.

/// Size in bytes of each Snowflake ID when serialized. Computed at compile time
/// from the actual ID type.
const SNOWFLAKE_ID_SIZE: usize = size_of::<<ServerIdType as Snowflake>::Ty>();

/// Size in bytes of packed ID data in each chunk. 4,096 IDs × 8 bytes =
/// 32,768 bytes (32 KiB) per chunk
const DEFAULT_CHUNK_BYTES: usize = DEFAULT_IDS_PER_CHUNK * SNOWFLAKE_ID_SIZE;

/// Number of chunks buffered in each response stream before blocking. Controls
/// how much data can be sent ahead of client consumption. Total buffer per
/// stream: 8 chunks × 32 KiB = 2 MiB
const DEFAULT_STREAM_BUFFER_SIZE: usize = 8;

/// Upper bound on total memory used by all active streams if all workers are
/// generating simultaneously: NUM_WORKERS × DEFAULT_STREAM_BUFFER_SIZE ×
/// DEFAULT_CHUNK_BYTES = 1024 × 8 × 32 KiB = 2 GiB theoretical maximum
///
/// In practice, this is much lower since:
/// - Not all workers generate simultaneously
/// - Chunks are consumed as they're produced
/// - Most requests don't utilize all workers

impl IdService {
    fn new(num_workers: usize) -> Self {
        let mut workers = Vec::with_capacity(num_workers);
        let clock = MonotonicClock::default();

        for worker_id in 0..num_workers {
            let (tx, mut rx) = mpsc::channel::<WorkRequest>(DEFAULT_WORK_REQUEST_BUFFER_SIZE);
            workers.push(tx);
            let generator = BasicSnowflakeGenerator::<ServerIdType, _>::new(
                (SHARD_OFFSET + worker_id) as u64,
                clock.clone(),
            );

            tokio::spawn(async move {
                while let Some(work) = rx.recv().await {
                    match work {
                        WorkRequest::Stream {
                            count,
                            tx,
                            cancelled,
                        } => {
                            let mut chunk_buf = [0u8; DEFAULT_CHUNK_BYTES];
                            let mut buf_pos = 0; // Current position in buffer
                            let mut generated = 0;

                            while generated < count {
                                match generator.try_next_id() {
                                    Ok(IdGenStatus::Ready { id }) => {
                                        generated += 1;

                                        // IMPORTANT: match LE bytes onthe client
                                        let id_bytes = id.to_raw().to_le_bytes();

                                        // Copy ID bytes into fixed buffer
                                        chunk_buf[buf_pos..buf_pos + SNOWFLAKE_ID_SIZE]
                                            .copy_from_slice(&id_bytes);
                                        buf_pos += SNOWFLAKE_ID_SIZE;

                                        // Check if buffer is full
                                        if buf_pos == DEFAULT_CHUNK_BYTES {
                                            if cancelled.is_cancelled() || tx.is_closed() {
                                                break;
                                            }

                                            // Send the full buffer
                                            let bytes = bytes::Bytes::copy_from_slice(&chunk_buf);
                                            if tx
                                                .send(Ok(IdUnitResponseChunk { packed_ids: bytes }))
                                                .await
                                                .is_err()
                                            {
                                                println!("Failed to send res");
                                                break;
                                            }

                                            // Reset buffer position for reuse
                                            buf_pos = 0;
                                        }
                                    }

                                    Ok(IdGenStatus::Pending { .. }) => {
                                        tokio::task::yield_now().await;
                                    }

                                    Err(e) => {
                                        if cancelled.is_cancelled() || tx.is_closed() {
                                            break;
                                        }
                                        if tx
                                            .send(Err(Status::internal(format!(
                                                "ID generation failed: {}",
                                                e
                                            ))))
                                            .await
                                            .is_err()
                                        {
                                            println!("Failed to send res");
                                            break;
                                        }
                                    }
                                }
                            }

                            // Send remaining partial buffer if not empty
                            if buf_pos > 0 && !cancelled.is_cancelled() && !tx.is_closed() {
                                let bytes = bytes::Bytes::copy_from_slice(&chunk_buf[..buf_pos]);
                                if tx
                                    .send(Ok(IdUnitResponseChunk { packed_ids: bytes }))
                                    .await
                                    .is_err()
                                {
                                    println!("Failed to send res");
                                }
                            }
                        }
                    }
                }
            });
        }

        Self {
            workers: Arc::new(workers),
        }
    }
}

#[tonic::async_trait]
impl IdGen for IdService {
    type GetStreamIdsStream =
        Pin<Box<dyn Stream<Item = Result<IdUnitResponseChunk, Status>> + Send>>;

    async fn get_stream_ids(
        &self,
        req: Request<IdStreamRequest>,
    ) -> Result<Response<Self::GetStreamIdsStream>, Status> {
        let cancellation_token = Arc::new(CancellationToken::new());
        let total_ids = req.get_ref().count as usize;
        let num_workers = self.workers.len();

        // Calculate number of chunks needed based on DEFAULT_CHUNK_SIZE
        let chunks_needed = (total_ids + DEFAULT_CHUNK_BYTES - 1) / DEFAULT_CHUNK_BYTES; // Ceiling division
        let mut streams = SelectAll::new();
        let mut rng = StdRng::from_rng(&mut rand::rng());
        let offset = rng.random_range(0..num_workers);

        let mut remaining_ids = total_ids;

        for chunk_idx in 0..chunks_needed {
            let chunk_size = std::cmp::min(remaining_ids, DEFAULT_CHUNK_BYTES);
            if chunk_size == 0 {
                break;
            }

            // Round-robin worker selection with some random offset
            let worker_idx = (offset + chunk_idx) % num_workers;
            let tx = &self.workers[worker_idx];

            let (resp_tx, resp_rx) = mpsc::channel(DEFAULT_STREAM_BUFFER_SIZE);
            tx.send(WorkRequest::Stream {
                count: chunk_size,
                tx: resp_tx,
                cancelled: cancellation_token.clone(),
            })
            .await
            .map_err(|e| Status::unavailable(format!("Service overloaded: {}", e)))?;

            streams.push(ReceiverStream::new(resp_rx));
            remaining_ids -= chunk_size;
        }

        let cancel_future = Box::pin(async move {
            cancellation_token.cancelled().await;
        });

        let stream = streams.take_until(cancel_future);
        Ok(Response::new(Box::pin(stream)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;
    println!(
        "Starting ID generation service with {} workers",
        NUM_WORKERS
    );

    let service = IdService::new(NUM_WORKERS);

    println!("gRPC ID service listening on {}", addr);

    Server::builder()
        .add_service(
            IdGenServer::new(service)
                .send_compressed(CompressionEncoding::Zstd)
                .accept_compressed(CompressionEncoding::Zstd),
        )
        .serve_with_shutdown(addr, async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C signal handler");
            println!("Shutdown signal received, terminating...");
        })
        .await?;

    Ok(())
}
