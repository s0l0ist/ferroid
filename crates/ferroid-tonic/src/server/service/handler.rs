//! gRPC service implementation for chunked Snowflake ID generation.
//!
//! This module defines the [`IdService`] struct, which implements the gRPC
//! [`IdGen`] service defined in the protobuf specification. It provides a
//! streaming endpoint that allows clients to request large batches of Snowflake
//! IDs in a cancellable, chunked fashion.
//!
//! ## Key Responsibilities
//!
//! - Spawn and manage a pool of background worker tasks.
//! - Validate and handle incoming `GetStreamIds` gRPC requests.
//! - Coordinate chunked ID generation via [`feed_chunks`].
//! - Support cancellation, backpressure, and graceful shutdown.
//!
//! ## Related Modules
//!
//! - [`crate::server::service::pool`] - worker pool and routing.
//! - [`crate::server::service::streaming`] - stream coordination and chunk
//!   processing.
use crate::{
    config::ServerConfig,
    pool::{manager::WorkerPool, worker::worker_loop},
    service::config::{Clock, Generator},
    streaming::coordinator::feed_chunks,
    telemetry::{
        decrement_streams_inflight, increment_ids_generated, increment_requests,
        increment_stream_errors, increment_streams_inflight, record_ids_per_request,
        record_stream_duration,
    },
};
use core::pin::Pin;
use ferroid::Snowflake;
use ferroid_tonic::common::{
    Error,
    idgen::{IdStreamRequest, IdUnitResponseChunk, id_gen_server::IdGen},
    types::{SNOWFLAKE_ID_SIZE, SnowflakeId},
};
use futures::TryStreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

/// gRPC service for distributed Snowflake ID generation.
///
/// This struct implements the [`IdGen`] trait defined in the protobuf service
/// definition. It processes streaming ID generation requests by dispatching
/// chunked work to a pool of background worker tasks.
#[derive(Clone)]
pub struct IdService {
    config: ServerConfig,
    worker_pool: Arc<WorkerPool>,
}

impl IdService {
    /// Initializes the service and spawns `num_workers` background tasks.
    ///
    /// Each worker is assigned a unique Snowflake machine ID and runs its own
    /// generator and bounded channel. All workers share a global shutdown
    /// token.
    pub fn new(config: ServerConfig) -> Self {
        let mut workers = Vec::with_capacity(config.num_workers);
        let clock = Clock::default();
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
                (config.shard_offset + worker_id) as <SnowflakeId as Snowflake>::Ty,
                clock.clone(),
            );

            tokio::spawn(worker_loop(worker_id, rx, generator, config.chunk_bytes));
        }

        let worker_pool = WorkerPool::new(workers, shutdown_token);

        Self {
            config,
            worker_pool: Arc::new(worker_pool),
        }
    }

    /// Initiates graceful shutdown of the worker pool.
    ///
    /// This cancels all in-flight streams and waits for each worker to
    /// acknowledge shutdown.
    pub async fn shutdown(&self) -> Result<(), Error> {
        self.worker_pool.shutdown().await
    }
}

#[tonic::async_trait]
impl IdGen for IdService {
    type GetStreamIdsStream =
        Pin<Box<dyn Stream<Item = Result<IdUnitResponseChunk, Status>> + Send>>;

    /// Handles a streaming ID generation request from the client.
    ///
    /// The total requested count is validated and split into fixed-size chunks.
    /// Each chunk is delegated to the worker pool. Cancellation is supported
    /// via a scoped [`CancellationToken`].
    #[cfg_attr(feature = "tracing", tracing::instrument(skip_all, fields(count = req.get_ref().count)))]
    async fn get_stream_ids(
        &self,
        req: Request<IdStreamRequest>,
    ) -> Result<Response<Self::GetStreamIdsStream>, Status> {
        let start = std::time::Instant::now();
        let total_ids = req.get_ref().count as usize;

        if total_ids == 0 {
            increment_stream_errors();
            return Err(Error::InvalidRequest {
                reason: "Count must be greater than 0".to_string(),
            }
            .into());
        }

        if total_ids > self.config.max_allowed_ids {
            increment_stream_errors();
            return Err(Error::InvalidRequest {
                reason: format!(
                    "Count {} exceeds maximum allowed ({})",
                    total_ids, self.config.max_allowed_ids
                ),
            }
            .into());
        }

        increment_requests();
        record_ids_per_request(total_ids as f64);
        increment_streams_inflight();

        let (resp_tx, resp_rx) =
            mpsc::channel::<Result<IdUnitResponseChunk, Status>>(self.config.stream_buffer_size);

        let worker_pool = Arc::clone(&self.worker_pool);
        let config = self.config.clone();

        let fut = async move {
            match feed_chunks(total_ids, worker_pool, resp_tx, config).await {
                Ok(_) => {
                    decrement_streams_inflight();
                    record_stream_duration(start.elapsed().as_millis() as f64);
                }
                Err(_e) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("Error: {}", _e);
                }
            }
        };
        #[cfg(feature = "tracing")]
        let fut = {
            use tracing::Instrument;
            let span = tracing::info_span!("streaming");
            let fut = fut.instrument(span);
            fut
        };

        tokio::spawn(fut);

        let stream = ReceiverStream::new(resp_rx)
            .inspect_ok(|chunk| {
                // packed_ids contains binary representation of the IDs,
                // therefore, we must divide by the size of the
                // snowflake ID to get the actual number of IDs
                // generated.
                increment_ids_generated((chunk.packed_ids.len() / SNOWFLAKE_ID_SIZE) as u64);
            })
            .inspect_err(move |_e| {
                increment_stream_errors();
            });

        Ok(Response::new(Box::pin(stream)))
    }
}
