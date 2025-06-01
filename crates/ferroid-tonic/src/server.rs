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

#[derive(Error, Debug)]
pub enum IdServiceError {
    #[error("Channel communication error: {context}")]
    ChannelError { context: String },

    #[error("ID generation failed: {0:?}")]
    IdGeneration(#[from] ferroid::Error),

    #[error("Service is overloaded: {details}")]
    ServiceOverloaded { details: String },

    #[error("Request cancelled by client")]
    RequestCancelled,

    #[error("Invalid request: {reason}")]
    InvalidRequest { reason: String },
}

impl From<IdServiceError> for Status {
    fn from(err: IdServiceError) -> Self {
        match err {
            IdServiceError::ChannelError { context } => {
                Status::unavailable(format!("Channel error: {}", context))
            }
            IdServiceError::IdGeneration(e) => {
                Status::internal(format!("ID generation error: {:?}", e))
            }
            IdServiceError::ServiceOverloaded { details } => Status::resource_exhausted(details),
            IdServiceError::RequestCancelled => Status::cancelled("Request was cancelled"),
            IdServiceError::InvalidRequest { reason } => Status::invalid_argument(reason),
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

const SHARD_OFFSET: usize = 0;
const NUM_WORKERS: usize = 512;
const _: () = assert!(NUM_WORKERS <= (SnowflakeIdType::max_machine_id() as usize) + 1);
const DEFAULT_WORK_REQUEST_BUFFER_SIZE: usize = 2048;
const DEFAULT_IDS_PER_CHUNK: usize = 2048;
const DEFAULT_CHUNK_BYTES: usize = DEFAULT_IDS_PER_CHUNK * SNOWFLAKE_ID_SIZE;
const DEFAULT_STREAM_BUFFER_SIZE: usize = 8;

impl IdService {
    #[instrument(name = "id_service_new", fields(num_workers = num_workers))]
    fn new(num_workers: usize) -> Self {
        info!("Initializing ID service with {} workers", num_workers);

        let mut workers = Vec::with_capacity(num_workers);
        let clock = MonotonicClock::default();

        for worker_id in 0..num_workers {
            let (tx, rx) = mpsc::channel::<WorkRequest>(DEFAULT_WORK_REQUEST_BUFFER_SIZE);
            workers.push(tx);
            let generator = BasicSnowflakeGenerator::<SnowflakeIdType, _>::new(
                (SHARD_OFFSET + worker_id) as <SnowflakeIdType as Snowflake>::Ty,
                clock.clone(),
            );
            tokio::spawn(worker_loop(worker_id, rx, generator));
        }

        Self {
            workers: Arc::new(workers),
        }
    }
}

#[instrument(name = "worker_loop", skip_all, fields(worker_id))]
async fn worker_loop(
    worker_id: usize,
    mut rx: mpsc::Receiver<WorkRequest>,
    generator: BasicSnowflakeGenerator<SnowflakeIdType, MonotonicClock>,
) {
    debug!("Worker {} started", worker_id);

    while let Some(work) = rx.recv().await {
        match work {
            WorkRequest::Stream {
                count,
                tx,
                cancelled,
            } => {
                let mut chunk_buf = [0_u8; DEFAULT_CHUNK_BYTES];
                let mut buf_pos = 0;
                let mut generated = 0;

                while generated < count {
                    match generator.try_next_id() {
                        Ok(IdGenStatus::Ready { id }) => {
                            generated += 1;
                            let id_bytes = id.to_raw().to_le_bytes();
                            chunk_buf[buf_pos..buf_pos + SNOWFLAKE_ID_SIZE]
                                .copy_from_slice(&id_bytes);
                            buf_pos += SNOWFLAKE_ID_SIZE;

                            if buf_pos == DEFAULT_CHUNK_BYTES {
                                if cancelled.is_cancelled() || tx.is_closed() {
                                    warn!("Worker {} cancelled or tx closed", worker_id);
                                    break;
                                }

                                let bytes = bytes::Bytes::copy_from_slice(&chunk_buf);
                                if let Err(e) =
                                    tx.send(Ok(IdUnitResponseChunk { packed_ids: bytes })).await
                                {
                                    error!("Worker {} failed to send chunk: {}", worker_id, e);
                                    break;
                                }
                                buf_pos = 0;
                            }
                        }
                        Ok(IdGenStatus::Pending { .. }) => {
                            tokio::task::yield_now().await;
                        }
                        Err(e) => {
                            if cancelled.is_cancelled() || tx.is_closed() {
                                warn!("Worker {} cancelled or tx closed", worker_id);
                                break;
                            }

                            if let Err(e) =
                                tx.send(Err(IdServiceError::IdGeneration(e).into())).await
                            {
                                error!("Worker {} failed to send error: {}", worker_id, e);
                                break;
                            }
                        }
                    }
                }

                if buf_pos > 0 && !cancelled.is_cancelled() && !tx.is_closed() {
                    let bytes = bytes::Bytes::copy_from_slice(&chunk_buf[..buf_pos]);
                    if let Err(e) = tx.send(Ok(IdUnitResponseChunk { packed_ids: bytes })).await {
                        error!("Worker {} failed to send final chunk: {}", worker_id, e);
                    }
                }
            }
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

        let workers = Arc::clone(&self.workers);
        let cancel = cancellation_token.clone();

        tokio::spawn(async move {
            match feed_chunks(total_ids, workers, resp_tx, cancel).await {
                Ok(()) => debug!("feed_chunks completed"),
                Err(e) => error!("feed_chunks error: {e:?}"),
            }
        });

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
) -> Result<(), IdServiceError> {
    let mut remaining = total_ids;
    let mut rng = StdRng::from_rng(&mut rand::rng());

    while remaining > 0 {
        let chunk_size = remaining.min(DEFAULT_IDS_PER_CHUNK);
        remaining -= chunk_size;

        let (chunk_tx, mut chunk_rx) = mpsc::channel(2);
        let worker_idx = rng.random_range(0..workers.len());
        let worker = &workers[worker_idx];

        if let Err(e) = worker
            .send(WorkRequest::Stream {
                count: chunk_size,
                tx: chunk_tx,
                cancelled: cancel.clone(),
            })
            .await
        {
            let err = IdServiceError::ChannelError {
                context: format!("Worker {worker_idx} send failed: {e}"),
            };

            if let Err(send_err) = resp_tx.send(Err(err.into())).await {
                return Err(IdServiceError::ChannelError {
                    context: format!("Response channel closed while sending error: {send_err}"),
                });
            }
        }

        while let Some(msg) = chunk_rx.recv().await {
            if let Err(e) = resp_tx.send(msg).await {
                return Err(IdServiceError::ChannelError {
                    context: format!("Failed to forward chunk to client: {e}"),
                });
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
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

    Server::builder()
        .http2_adaptive_window(Some(true))
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
