//! gRPC service implementation for Snowflake ID generation.
//!
//! This module defines the [`IdService`] struct, which implements the gRPC
//! [`IdGen`] service defined in `proto/idgen.proto`. It supports cancellable,
//! chunked streaming of Snowflake IDs using a pool of background workers.
//!
//! ## Responsibilities
//! - Spawn a worker pool, each with a unique generator.
//! - Handle incoming `GetStreamIds` gRPC requests.
//! - Forward ID generation work to the worker pool.
//! - Return results to the client as a cancellable gRPC stream.
//!
//! ## Related Modules
//! - [`crate::service::worker`] — Worker implementation logic.
//! - [`crate::service::pool`] — Worker pool and routing logic.
//! - [`crate::service::stream`] — Chunk buffering and stream forwarding.

use super::{pool::WorkerPool, request::WorkRequest, stream::feed_chunks, worker::worker_loop};
use crate::{
    common::SnowflakeIdType,
    config::{
        ClockType, DEFAULT_STREAM_BUFFER_SIZE, DEFAULT_WORK_REQUEST_BUFFER_SIZE, MAX_ALLOWED_IDS,
        SHARD_OFFSET, SnowflakeGeneratorType,
    },
    error::IdServiceError,
    idgen::{IdStreamRequest, IdUnitResponseChunk, id_gen_server::IdGen},
};
use core::pin::Pin;
use ferroid::Snowflake;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};

/// gRPC ID generation service.
///
/// Owns a [`WorkerPool`] and implements [`IdGen`] to handle chunked streaming
/// of Snowflake IDs. Each request is split into fixed-size chunks that are
/// asynchronously processed by worker tasks.
#[derive(Clone)]
pub struct IdService {
    worker_pool: Arc<WorkerPool>,
}

impl IdService {
    /// Initializes the gRPC service and spawns `num_workers` background
    /// generators.
    ///
    /// Each worker is initialized with a unique machine ID and receives its own
    /// MPSC queue. All workers share a common shutdown token.
    #[cfg_attr(feature = "tracing", tracing::instrument(name = "id_service_new", fields(num_workers = num_workers)))]
    pub fn new(num_workers: usize) -> Self {
        #[cfg(feature = "tracing")]
        tracing::info!("Initializing ID service with {} workers", num_workers);

        let mut workers = Vec::with_capacity(num_workers);
        let clock = ClockType::default();
        let shutdown_token = CancellationToken::new();

        for worker_id in 0..num_workers {
            let (tx, rx) = mpsc::channel::<WorkRequest>(DEFAULT_WORK_REQUEST_BUFFER_SIZE);
            workers.push(tx);

            let generator = SnowflakeGeneratorType::new(
                (SHARD_OFFSET + worker_id) as <SnowflakeIdType as Snowflake>::Ty,
                clock.clone(),
            );

            let worker_shutdown_token = shutdown_token.clone();
            tokio::spawn(worker_loop(worker_id, rx, generator, worker_shutdown_token));
        }

        let worker_pool = Arc::new(WorkerPool::new(Arc::new(workers), shutdown_token));

        Self { worker_pool }
    }

    /// Initiates a graceful shutdown of all worker tasks.
    pub async fn shutdown(&self) -> Result<(), IdServiceError> {
        self.worker_pool.shutdown().await
    }
}

#[tonic::async_trait]
impl IdGen for IdService {
    type GetStreamIdsStream =
        Pin<Box<dyn Stream<Item = Result<IdUnitResponseChunk, Status>> + Send>>;

    /// Handles the `GetStreamIds` gRPC call.
    ///
    /// Validates request count, spawns a background task to generate IDs in
    /// chunks, and returns a stream of chunks to the client. Cancellation is
    /// supported via [`CancellationToken`].
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
