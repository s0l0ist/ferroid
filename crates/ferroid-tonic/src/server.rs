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
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
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
    #[error("Service is shutting down")]
    ServiceShutdown,
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
            IdServiceError::ServiceShutdown => Status::unavailable("Service is shutting down"),
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
    Shutdown {
        response: oneshot::Sender<()>,
    },
}

struct WorkerPool {
    workers: Arc<Vec<mpsc::Sender<WorkRequest>>>,
    next_worker: AtomicUsize,
    shutdown_token: CancellationToken,
}

impl WorkerPool {
    fn next_worker_index(&self) -> usize {
        self.next_worker.fetch_add(1, Ordering::Relaxed) % self.workers.len()
    }

    async fn send_to_next_worker(&self, request: WorkRequest) -> Result<(), IdServiceError> {
        if self.shutdown_token.is_cancelled() {
            return Err(IdServiceError::ServiceShutdown);
        }

        let worker_idx = self.next_worker_index();
        let worker = &self.workers[worker_idx];

        // Try with backpressure - if channel is full, try next worker
        match worker.try_send(request) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(request)) => {
                // Try to send with timeout as fallback
                match tokio::time::timeout(Duration::from_millis(100), worker.send(request)).await {
                    Ok(Ok(())) => Ok(()),
                    Ok(Err(_)) => Err(IdServiceError::ChannelError {
                        context: format!("Worker {} channel closed", worker_idx),
                    }),
                    Err(_) => Err(IdServiceError::ServiceOverloaded {
                        details: format!("Worker {} timeout after 100ms", worker_idx),
                    }),
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => Err(IdServiceError::ChannelError {
                context: format!("Worker {} channel closed", worker_idx),
            }),
        }
    }

    async fn shutdown(&self) -> Result<(), IdServiceError> {
        info!("Initiating worker pool shutdown");
        self.shutdown_token.cancel();

        let mut shutdown_handles = Vec::new();

        // Send shutdown signal to all workers
        for (i, worker) in self.workers.iter().enumerate() {
            let (tx, rx) = oneshot::channel();
            if let Err(e) = worker.send(WorkRequest::Shutdown { response: tx }).await {
                warn!("Failed to send shutdown signal to worker {}: {}", i, e);
            } else {
                shutdown_handles.push(rx);
            }
        }

        // Wait for all workers to acknowledge shutdown
        for (i, handle) in shutdown_handles.into_iter().enumerate() {
            match tokio::time::timeout(Duration::from_secs(5), handle).await {
                Ok(Ok(())) => debug!("Worker {} shut down gracefully", i),
                Ok(Err(_)) => warn!("Worker {} shutdown response channel closed", i),
                Err(_) => warn!("Worker {} shutdown timeout", i),
            }
        }

        info!("Worker pool shutdown complete");
        Ok(())
    }
}

#[derive(Clone)]
struct IdService {
    worker_pool: Arc<WorkerPool>,
}

// Constants
const SHARD_OFFSET: usize = 0;
const NUM_WORKERS: usize = 512;
const MAX_ALLOWED_IDS: usize = 1_000_000_000;
const DEFAULT_WORK_REQUEST_BUFFER_SIZE: usize = 2048;
const DEFAULT_IDS_PER_CHUNK: usize = 2048;
const DEFAULT_CHUNK_BYTES: usize = DEFAULT_IDS_PER_CHUNK * SNOWFLAKE_ID_SIZE;
const DEFAULT_STREAM_BUFFER_SIZE: usize = 8;

// Compile-time assertion
const _: () = assert!(NUM_WORKERS <= (SnowflakeIdType::max_machine_id() as usize) + 1);

impl IdService {
    #[instrument(name = "id_service_new", fields(num_workers = num_workers))]
    fn new(num_workers: usize) -> Self {
        info!("Initializing ID service with {} workers", num_workers);
        let mut workers = Vec::with_capacity(num_workers);
        let clock = MonotonicClock::default();
        let shutdown_token = CancellationToken::new();

        for worker_id in 0..num_workers {
            let (tx, rx) = mpsc::channel::<WorkRequest>(DEFAULT_WORK_REQUEST_BUFFER_SIZE);
            workers.push(tx);

            let generator = BasicSnowflakeGenerator::<SnowflakeIdType, _>::new(
                (SHARD_OFFSET + worker_id) as <SnowflakeIdType as Snowflake>::Ty,
                clock.clone(),
            );

            let worker_shutdown_token = shutdown_token.clone();
            tokio::spawn(worker_loop(worker_id, rx, generator, worker_shutdown_token));
        }

        let worker_pool = Arc::new(WorkerPool {
            workers: Arc::new(workers),
            next_worker: AtomicUsize::new(0),
            shutdown_token,
        });

        Self { worker_pool }
    }

    async fn shutdown(&self) -> Result<(), IdServiceError> {
        self.worker_pool.shutdown().await
    }
}

// Helper function to check if we should stop processing
fn should_stop(
    cancelled: &CancellationToken,
    tx: &mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
) -> bool {
    tx.is_closed() || cancelled.is_cancelled()
}

// #[instrument(name = "worker_loop", skip_all, fields(worker_id))]
async fn worker_loop(
    worker_id: usize,
    mut rx: mpsc::Receiver<WorkRequest>,
    mut generator: BasicSnowflakeGenerator<SnowflakeIdType, MonotonicClock>,
    shutdown_token: CancellationToken,
) {
    debug!("Worker {} started", worker_id);

    loop {
        tokio::select! {
            work = rx.recv() => {
                match work {
                    Some(WorkRequest::Stream { count, tx, cancelled }) => {
                        process_stream_request(worker_id, count, tx, cancelled, &mut generator).await;
                    }
                    Some(WorkRequest::Shutdown { response }) => {
                        debug!("Worker {} received shutdown signal", worker_id);
                        let _ = response.send(());
                        break;
                    }
                    None => {
                        debug!("Worker {} channel closed", worker_id);
                        break;
                    }
                }
            }
            _ = shutdown_token.cancelled() => {
                debug!("Worker {} shutdown via cancellation token", worker_id);
                break;
            }
        }
    }

    debug!("Worker {} stopped", worker_id);
}

async fn process_stream_request(
    worker_id: usize,
    count: usize,
    tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    cancelled: Arc<CancellationToken>,
    generator: &mut BasicSnowflakeGenerator<SnowflakeIdType, MonotonicClock>,
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
                        debug!("Worker {} stopping before chunk send", worker_id);
                        return;
                    }

                    let bytes = bytes::Bytes::copy_from_slice(&chunk_buf);
                    if let Err(e) = tx.send(Ok(IdUnitResponseChunk { packed_ids: bytes })).await {
                        debug!("Worker {} failed to send chunk: {}", worker_id, e);
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
                    debug!("Worker {} stopping after ID generation error", worker_id);
                    return;
                }

                if let Err(send_err) = tx.send(Err(IdServiceError::IdGeneration(e).into())).await {
                    debug!("Worker {} failed to send error: {}", worker_id, send_err);
                }
                return;
            }
        }
    }

    // Send final partial chunk if needed
    if buf_pos > 0 && !should_stop(&cancelled, &tx) {
        let bytes = bytes::Bytes::copy_from_slice(&chunk_buf[..buf_pos]);
        if let Err(e) = tx.send(Ok(IdUnitResponseChunk { packed_ids: bytes })).await {
            debug!("Worker {} failed to send final chunk: {}", worker_id, e);
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

        if total_ids > MAX_ALLOWED_IDS {
            return Err(IdServiceError::InvalidRequest {
                reason: format!(
                    "Count {} exceeds maximum allowed ({})",
                    total_ids, MAX_ALLOWED_IDS
                ),
            }
            .into());
        }

        let cancellation_token = Arc::new(CancellationToken::new());
        let (resp_tx, resp_rx) =
            mpsc::channel::<Result<IdUnitResponseChunk, Status>>(DEFAULT_STREAM_BUFFER_SIZE);

        let worker_pool = Arc::clone(&self.worker_pool);
        let cancel = cancellation_token.clone();

        tokio::spawn(async move {
            feed_chunks(total_ids, worker_pool, resp_tx, cancel).await;
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
    worker_pool: Arc<WorkerPool>,
    resp_tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
    cancel: Arc<CancellationToken>,
) {
    let mut remaining = total_ids;

    while remaining > 0 {
        if cancel.is_cancelled() {
            debug!("feed_chunks cancelled");
            break;
        }

        let chunk_size = remaining.min(DEFAULT_IDS_PER_CHUNK);
        remaining -= chunk_size;

        let (chunk_tx, mut chunk_rx) = mpsc::channel(2);

        // Send work request to worker pool with proper error handling
        match worker_pool
            .send_to_next_worker(WorkRequest::Stream {
                count: chunk_size,
                tx: chunk_tx,
                cancelled: cancel.clone(),
            })
            .await
        {
            Ok(()) => {
                // Forward all messages from worker to response channel
                while let Some(msg) = chunk_rx.recv().await {
                    if let Err(e) = resp_tx.send(msg).await {
                        debug!("Response channel failed to forward chunk: {}", e);
                        return;
                    }
                }
            }
            Err(e) => {
                error!("Failed to send work to worker: {:?}", e);
                if let Err(e) = resp_tx.send(Err(e.into())).await {
                    debug!("Response channel failed to forward error: {}", e);
                }
                return;
            }
        }
    }
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
        max_allowed_ids = MAX_ALLOWED_IDS,
        "Starting ID generation service"
    );

    let service = IdService::new(NUM_WORKERS);
    let service_for_shutdown = service.clone();

    let server = Server::builder()
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

            if let Err(e) = service_for_shutdown.shutdown().await {
                error!("Error during service shutdown: {:?}", e);
            }
        });

    if let Err(e) = server.await {
        error!("Server error: {:?}", e);
        return Err(e.into());
    }

    info!("Service shut down successfully");
    Ok(())
}
