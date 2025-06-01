//! gRPC ID generation service using ferroid and tonic. Provides single, batch,
//! and streaming ID generation via Snowflake-like IDs with cancellation
//! support.

use core::pin::Pin;
use ferroid::{BasicSnowflakeGenerator, IdGenStatus, MonotonicClock, Snowflake};
use futures::StreamExt;
use idgen::{
    IdStreamRequest, IdUnitResponseChunk,
    id_gen_server::{IdGen, IdGenServer},
};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status, codec::CompressionEncoding, transport::Server};
use tracing::{debug, error, info, instrument, warn};

pub mod common;
use common::{SNOWFLAKE_ID_SIZE, SnowflakeIdType};

pub mod idgen {
    tonic::include_proto!("idgen");
}

/// Custom error types for the ID generation service
#[derive(Error, Debug)]
pub enum IdServiceError {
    #[error("Worker channel send failed: {message}")]
    WorkerChannelSend { message: String },

    #[error("Response channel send failed: {message}")]
    ResponseChannelSend { message: String },

    #[error("ID generation failed: {0}")]
    IdGeneration(#[from] ferroid::Error),

    #[error("Service is overloaded: {details}")]
    ServiceOverloaded { details: String },

    #[error("Request cancelled by client")]
    RequestCancelled,

    #[error("Invalid request: {reason}")]
    InvalidRequest { reason: String },

    #[error("Worker task failure: {worker_id}, reason: {reason}")]
    WorkerTaskFailure { worker_id: usize, reason: String },

    #[error("Stream buffer overflow: {buffer_size} exceeded")]
    StreamBufferOverflow { buffer_size: usize },
}

impl From<IdServiceError> for Status {
    fn from(err: IdServiceError) -> Self {
        match err {
            IdServiceError::WorkerChannelSend { message } => {
                Status::unavailable(format!("Worker unavailable: {}", message))
            }
            IdServiceError::ResponseChannelSend { message } => {
                Status::internal(format!("Response delivery failed: {}", message))
            }
            IdServiceError::IdGeneration(err) => {
                Status::internal(format!("ID generation error: {}", err))
            }
            IdServiceError::ServiceOverloaded { details } => {
                Status::resource_exhausted(format!("Service overloaded: {}", details))
            }
            IdServiceError::RequestCancelled => Status::cancelled("Request was cancelled"),
            IdServiceError::InvalidRequest { reason } => {
                Status::invalid_argument(format!("Invalid request: {}", reason))
            }
            IdServiceError::WorkerTaskFailure { worker_id, reason } => {
                Status::internal(format!("Worker {} failed: {}", worker_id, reason))
            }
            IdServiceError::StreamBufferOverflow { buffer_size } => {
                Status::resource_exhausted(format!("Stream buffer overflow: {}", buffer_size))
            }
        }
    }
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

/// The offset to shard the generators
const SHARD_OFFSET: usize = 0;

/// Number of worker tasks processing ID generation requests concurrently. Each
/// worker handles one generation request at a time but can process multiple
/// chunks within that request.
const NUM_WORKERS: usize = 1024;

/// Buffer capacity for each worker's mpsc channel. Controls how many work
/// requests can be queued per worker before backpressure kicks in.
const DEFAULT_WORK_REQUEST_BUFFER_SIZE: usize = 1;

/// Number of Snowflake IDs packed into each response chunk.
const DEFAULT_IDS_PER_CHUNK: usize = 2048;

/// Size in bytes of packed ID data in each chunk.
const DEFAULT_CHUNK_BYTES: usize = DEFAULT_IDS_PER_CHUNK * SNOWFLAKE_ID_SIZE;

/// Number of chunks buffered in each response stream before blocking.
const DEFAULT_STREAM_BUFFER_SIZE: usize = 8;

impl IdService {
    #[instrument(name = "id_service_new", fields(num_workers = num_workers))]
    fn new(num_workers: usize) -> Self {
        info!("Initializing ID service with {} workers", num_workers);

        let mut workers = Vec::with_capacity(num_workers);
        let clock = MonotonicClock::default();

        for worker_id in 0..num_workers {
            let (tx, mut rx) = mpsc::channel::<WorkRequest>(DEFAULT_WORK_REQUEST_BUFFER_SIZE);
            workers.push(tx);
            let generator = BasicSnowflakeGenerator::<SnowflakeIdType, _>::new(
                (SHARD_OFFSET + worker_id) as <SnowflakeIdType as Snowflake>::Ty,
                clock.clone(),
            );

            // Spawn worker task with proper error handling
            tokio::spawn(async move {
                debug!("Worker {} started", worker_id);

                while let Some(work) = rx.recv().await {
                    match work {
                        WorkRequest::Stream {
                            count,
                            tx,
                            cancelled,
                        } => {
                            let mut chunk_buf = [0_u8; DEFAULT_CHUNK_BYTES];
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
                                            if cancelled.is_cancelled() {
                                                warn!("Worker {} cancelled", worker_id);
                                                break;
                                            }
                                            if tx.is_closed() {
                                                warn!("Worker {} closed", worker_id);
                                                break;
                                            }

                                            // Send the full buffer
                                            let bytes = bytes::Bytes::copy_from_slice(&chunk_buf);
                                            if let Err(e) = tx
                                                .send(Ok(IdUnitResponseChunk { packed_ids: bytes }))
                                                .await
                                            {
                                                warn!(
                                                    "Worker {} failed to send res: {}",
                                                    worker_id, e
                                                );

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
                                        if cancelled.is_cancelled() {
                                            warn!("Worker {} cancelled", worker_id);
                                            break;
                                        }
                                        if tx.is_closed() {
                                            warn!("Worker {} closed", worker_id);
                                            break;
                                        }
                                        if let Err(e) = tx
                                            .send(Err(Status::internal(format!(
                                                "ID generation failed: {}",
                                                e
                                            ))))
                                            .await
                                        {
                                            warn!("Worker {} failed to send err: {}", worker_id, e);
                                            break;
                                        }
                                    }
                                }
                            }

                            // Send remaining partial buffer if not empty
                            if buf_pos > 0 && !cancelled.is_cancelled() && !tx.is_closed() {
                                let bytes = bytes::Bytes::copy_from_slice(&chunk_buf[..buf_pos]);
                                if let Err(e) =
                                    tx.send(Ok(IdUnitResponseChunk { packed_ids: bytes })).await
                                {
                                    error!("Worker {} failed to send res: {}", worker_id, e);
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

    #[instrument(name = "get_stream_ids", skip_all, fields(count = req.get_ref().count))]
    async fn get_stream_ids(
        &self,
        req: Request<IdStreamRequest>,
    ) -> Result<Response<Self::GetStreamIdsStream>, Status> {
        let total_ids = req.get_ref().count as usize;
        info!(total_ids = total_ids, "Received stream request");

        // Validate request
        if total_ids == 0 {
            return Err(IdServiceError::InvalidRequest {
                reason: "Count must be greater than 0".to_string(),
            }
            .into());
        }

        if total_ids > 1_000_000_000 {
            return Err(IdServiceError::InvalidRequest {
                reason: format!("Count {} exceeds maximum allowed (1B)", total_ids),
            }
            .into());
        }

        let cancellation_token = Arc::new(CancellationToken::new());
        let (resp_tx, resp_rx) =
            mpsc::channel::<Result<IdUnitResponseChunk, Status>>(DEFAULT_STREAM_BUFFER_SIZE);

        // Spawn the per-stream chunk feeder
        let workers = Arc::clone(&self.workers);
        let cancel = cancellation_token.clone();
        tokio::spawn(feed_chunks(total_ids, workers, resp_tx, cancel));

        let cancel_future = Box::pin(async move {
            cancellation_token.cancelled().await;
        });

        let stream = ReceiverStream::new(resp_rx).take_until(cancel_future);
        Ok(Response::new(Box::pin(stream)))
    }
}

async fn feed_chunks(
    total_ids: usize,
    workers: Arc<Vec<mpsc::Sender<WorkRequest>>>,
    resp_tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    cancel: Arc<CancellationToken>,
) {
    let mut remaining = total_ids;
    let mut rng = StdRng::from_rng(&mut rand::rng());

    while remaining > 0 {
        let chunk_size = remaining.min(DEFAULT_IDS_PER_CHUNK);
        remaining -= chunk_size;

        let (chunk_tx, mut chunk_rx) = mpsc::channel(2);
        let worker_idx = rng.random_range(0..workers.len());
        let worker = &workers[worker_idx];

        // Try dispatching work
        if let Err(e) = worker
            .send(WorkRequest::Stream {
                count: chunk_size,
                tx: chunk_tx,
                cancelled: cancel.clone(),
            })
            .await
        {
            error!("Failed to dispatch to worker {worker_idx}: {e}");

            if let Err(e) = resp_tx
                .send(Err(Status::unavailable("Worker send failed")))
                .await
            {
                error!("Failed to forward err to client: {e}");
                return;
            }

            return;
        }

        // Try receiving work
        while let Some(msg) = chunk_rx.recv().await {
            if let Err(e) = resp_tx.send(msg).await {
                error!("Failed to forward chunk to client: {e}");
                return;
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
        .pretty()
        .init();
    let addr = "127.0.0.1:50051".parse()?;

    info!(
        workers = NUM_WORKERS,
        address = %addr,
        chunk_size = DEFAULT_IDS_PER_CHUNK,
        buffer_size = DEFAULT_WORK_REQUEST_BUFFER_SIZE,
        "Starting ID generation service"
    );

    let service = IdService::new(NUM_WORKERS);

    info!("gRPC ID service listening on {}", addr);

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
            info!("Shutdown signal received, terminating gracefully...");
        })
        .await?;

    info!("Service shut down successfully");
    Ok(())
}
