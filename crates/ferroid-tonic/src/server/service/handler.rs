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

use super::pool::manager::WorkerPool;
use crate::{
    common::{error::IdServiceError, types::SnowflakeIdType},
    idgen::{IdStreamRequest, IdUnitResponseChunk, id_gen_server::IdGen},
    server::service::{
        config::{
            ClockType, DEFAULT_STREAM_BUFFER_SIZE, DEFAULT_WORK_REQUEST_BUFFER_SIZE,
            MAX_ALLOWED_IDS, SHARD_OFFSET, SnowflakeGeneratorType,
        },
        pool::worker::worker_loop,
        streaming::coordinator::feed_chunks,
    },
};
use core::pin::Pin;
use ferroid::Snowflake;
use futures::StreamExt;
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
    worker_pool: Arc<WorkerPool>,
}

impl IdService {
    /// Initializes the service and spawns `num_workers` background tasks.
    ///
    /// Each worker is assigned a unique Snowflake machine ID and runs its own
    /// generator and bounded channel. All workers share a global shutdown
    /// token.
    #[cfg_attr(feature = "tracing", tracing::instrument(name = "id_service_new", fields(num_workers = num_workers)))]
    pub fn new(num_workers: usize) -> Self {
        #[cfg(feature = "tracing")]
        tracing::info!("Initializing ID service with {} workers", num_workers);

        let mut workers = Vec::with_capacity(num_workers);
        let clock = ClockType::default();
        let shutdown_token = CancellationToken::new();

        for worker_id in 0..num_workers {
            let (tx, rx) = mpsc::channel(DEFAULT_WORK_REQUEST_BUFFER_SIZE);
            workers.push(tx);

            let generator = SnowflakeGeneratorType::new(
                (SHARD_OFFSET + worker_id) as <SnowflakeIdType as Snowflake>::Ty,
                clock.clone(),
            );

            tokio::spawn(worker_loop(worker_id, rx, generator));
        }

        let worker_pool = Arc::new(WorkerPool::new(Arc::new(workers), shutdown_token));

        Self { worker_pool }
    }

    /// Initiates graceful shutdown of the worker pool.
    ///
    /// This cancels all in-flight streams and waits for each worker to
    /// acknowledge shutdown.
    pub async fn shutdown(&self) -> Result<(), IdServiceError> {
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
    #[cfg_attr(feature = "tracing", tracing::instrument(name = "get_stream_ids", skip_all, fields(count = req.get_ref().count)))]
    async fn get_stream_ids(
        &self,
        req: Request<IdStreamRequest>,
    ) -> Result<Response<Self::GetStreamIdsStream>, Status> {
        let total_ids = req.get_ref().count as usize;

        #[cfg(feature = "tracing")]
        tracing::info!(total_ids = total_ids, "Received stream request");

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
