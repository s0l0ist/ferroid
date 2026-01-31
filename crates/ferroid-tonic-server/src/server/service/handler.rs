//! gRPC service implementation for chunked Snowflake ID generation.
//!
//! This module defines [`IdService`], the concrete implementation of the
//! [`IdGenerator`] gRPC service defined in the protobuf specification. It
//! exposes a high-throughput streaming endpoint that allows clients to request
//! large batches of Snowflake-style IDs in a cancellable and chunked fashion.
//!
//! ## Responsibilities
//!
//! - Spawn and manage a background worker pool for ID generation.
//! - Validate incoming `StreamIds` requests and enforce limits.
//! - Dispatch ID generation tasks across workers via [`feed_chunks`].
//! - Handle backpressure, cancellation, and graceful shutdown.

use core::pin::Pin;
use std::sync::Arc;

use ferroid_tonic_core::{
    Error,
    ferroid::id::Id,
    proto::{IdChunk, StreamIdsRequest, id_generator_server::IdGenerator},
    types::{Clock, EPOCH, Generator, SNOWFLAKE_ID_SIZE, SnowflakeId},
};
use futures::TryStreamExt;
use portable_atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

use crate::server::{
    config::ServerConfig,
    pool::{manager::WorkerPool, worker::worker_loop},
    streaming::coordinator::feed_chunks,
    telemetry::{
        decrement_streams_inflight_metric, increment_ids_generated_metric,
        increment_requests_metric, increment_stream_errors_metric,
        increment_streams_inflight_metric, record_ids_per_request_metric,
        record_stream_duration_metric,
    },
};

// Global flag indicating shutdown has been initiated.
// Used to refuse new incoming requests.
static SHUTDOWN: AtomicBool = AtomicBool::new(false);
pub fn set_global_shutdown() {
    SHUTDOWN.store(true, Ordering::Relaxed);
}
pub fn get_global_shutdown() -> bool {
    SHUTDOWN.load(Ordering::Relaxed)
}

// Global counter tracking the number of active in-flight streams.
// Used to coordinate graceful shutdown.
static STREAMS_INFLIGHT: AtomicU64 = AtomicU64::new(0);
pub fn increment_streams_inflight() {
    STREAMS_INFLIGHT.fetch_add(1, Ordering::Relaxed);
}
pub fn decrement_streams_inflight() {
    STREAMS_INFLIGHT.fetch_sub(1, Ordering::Relaxed);
}
pub fn get_streams_inflight() -> u64 {
    STREAMS_INFLIGHT.load(Ordering::Relaxed)
}

/// High-throughput gRPC service for distributed Snowflake ID generation.
///
/// Implements the [`IdGenerator`] service defined in the protobuf schema. This
/// service enables clients to request large batches of unique Snowflake IDs
/// using a stream-based interface. Internally, it dispatches chunked work to a
/// pool of asynchronous background worker tasks.
///
/// Each client request is broken into fixed-size chunks and streamed back to
/// the client in binary-encoded form (`IdChunk`).
#[derive(Clone)]
pub struct IdService {
    config: ServerConfig,
    worker_pool: Arc<WorkerPool>,
}

impl IdService {
    /// Creates a new `IdService` and spawns a pool of background worker tasks.
    ///
    /// Each worker is assigned a unique machine ID and maintains its own
    /// bounded channel. Workers operate independently and communicate through
    /// MPSC queues.
    ///
    /// This service uses a *sequential dispatch* model, where each worker
    /// handles one chunk at a time. This avoids overloading workers and enables
    /// efficient batching, resulting in predictable throughput and low memory
    /// pressure.
    pub fn new(config: ServerConfig) -> Self {
        let mut workers = Vec::with_capacity(config.num_workers);
        let clock = Clock::with_epoch(EPOCH);
        let shutdown_token = CancellationToken::new();

        for worker_id in 0..config.num_workers {
            // We only send a single WorkRequest to a worker at a time, even
            // when processing large client requests. This is because
            // `feed_chunks()` dispatches chunks sequentially across workers
            // rather than flooding them concurrently.
            //
            // In this sequential mode, each worker receives one request,
            // processes it fully, sends back chunked responses via a separate
            // channel, and only then receives the next request.
            //
            // If we were instead dispatching multiple chunks concurrently to
            // the same worker - either by spawning tasks or using
            // `futures::join_all` - we would need a higher
            // `work_request_buffer_size` to avoid backpressure stalls. But in
            // practice, doing so degrades performance due to task overhead,
            // increased contention, and less effective batching.
            //
            // Empirically, a buffer size of 1 is optimal in this model: it
            // ensures at-most-one in-flight WorkRequest per worker, avoids
            // unnecessary memory usage, and maintains high throughput with
            // predictable latency.
            let (tx, rx) = mpsc::channel(1);
            workers.push(tx);

            let generator = Generator::new(
                (config.shard_offset + worker_id) as <SnowflakeId as Id>::Ty,
                clock.clone(),
            );

            tokio::spawn(worker_loop(worker_id, rx, generator, config.chunk_bytes));
        }

        let worker_pool = WorkerPool::new(workers, shutdown_token, config.shutdown_timeout);

        Self {
            config,
            worker_pool: Arc::new(worker_pool),
        }
    }

    /// Initiates a graceful shutdown of the worker pool.
    ///
    /// All in-flight requests are cancelled, and the shutdown process blocks
    /// until each worker acknowledges termination.
    pub async fn shutdown(&self) -> Result<(), Error> {
        self.worker_pool.shutdown().await
    }
}

#[tonic::async_trait]
impl IdGenerator for IdService {
    type StreamIdsStream = Pin<Box<dyn Stream<Item = Result<IdChunk, Status>> + Send>>;

    /// Handles a streaming request for Snowflake IDs.
    ///
    /// Validates the requested count, splits it into fixed-size chunks, and
    /// streams binary-packed responses (`IdChunk`) back to the client. Chunks
    /// are processed sequentially across worker threads.
    ///
    /// If `tracing` is enabled, spans are instrumented per request for
    /// observability.
    ///
    /// If `metrics` is enabled, emits telemetry for:
    /// - request rate
    /// - number of IDs requested/generated
    /// - concurrent stream count
    /// - stream duration
    /// - stream errors
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all, fields(count = req.get_ref().count)))]
    async fn stream_ids(
        &self,
        req: Request<StreamIdsRequest>,
    ) -> Result<Response<Self::StreamIdsStream>, Status> {
        if get_global_shutdown() {
            return Err(Error::ServiceShutdown.into());
        }
        let start = std::time::Instant::now();

        #[allow(clippy::cast_possible_truncation)]
        let total_ids = req.get_ref().count as usize;

        if total_ids == 0 {
            increment_stream_errors_metric();
            return Err(Error::InvalidRequest {
                reason: "Count must be greater than 0".to_string(),
            }
            .into());
        }

        if total_ids > self.config.max_allowed_ids {
            increment_stream_errors_metric();
            return Err(Error::InvalidRequest {
                reason: format!(
                    "Count {} exceeds maximum allowed ({})",
                    total_ids, self.config.max_allowed_ids
                ),
            }
            .into());
        }

        increment_requests_metric();
        record_ids_per_request_metric(req.get_ref().count);
        increment_streams_inflight(); // global
        increment_streams_inflight_metric();

        let (resp_tx, resp_rx) =
            mpsc::channel::<Result<IdChunk, Status>>(self.config.stream_buffer_size);

        let worker_pool = Arc::clone(&self.worker_pool);
        let config = self.config.clone();

        let fut = async move {
            if let Err(_e) = feed_chunks(total_ids, worker_pool, resp_tx, config).await {
                #[cfg(feature = "tracing")]
                tracing::warn!("Error: {}", _e);
            }
            decrement_streams_inflight(); // global
            decrement_streams_inflight_metric();

            #[allow(clippy::cast_precision_loss)]
            record_stream_duration_metric(start.elapsed().as_millis() as f64);
        };
        #[cfg(feature = "tracing")]
        let fut = {
            use tracing::Instrument;
            let span = tracing::info_span!("streaming");
            fut.instrument(span)
        };

        tokio::spawn(fut);

        let stream = ReceiverStream::new(resp_rx)
            .inspect_ok(|chunk| {
                // packed_ids contains binary representation of the IDs,
                // therefore, we must divide by the size of the snowflake ID to
                // get the actual number of IDs generated.
                increment_ids_generated_metric((chunk.packed_ids.len() / SNOWFLAKE_ID_SIZE) as u64);
            })
            .inspect_err(move |_e| {
                increment_stream_errors_metric();
            });

        Ok(Response::new(Box::pin(stream)))
    }
}
