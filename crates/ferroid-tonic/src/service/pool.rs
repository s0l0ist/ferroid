//! Worker pool abstraction for load-balanced and cancellable Snowflake ID
//! generation.
//!
//! This module defines the [`WorkerPool`] struct, which manages a fixed set of
//! Tokio tasks, each responsible for generating Snowflake IDs. The pool
//! enables:
//!
//! - Load-balanced distribution of work requests via round-robin routing.
//! - Backpressure-aware channel handling to avoid memory blow-up.
//! - Cooperative cancellation via a shared [`CancellationToken`].
//! - Graceful shutdown coordination through one-shot channels.
//!
//! The pool is used internally by the gRPC `IdService` to delegate streaming or
//! batched ID generation tasks to worker tasks.

use super::request::WorkRequest;
use crate::error::IdServiceError;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// A pool of worker tasks that generate Snowflake IDs.
///
/// Each worker is a Tokio task running a `worker_loop` and listening on its own
/// MPSC channel. This pool maintains round-robin routing and graceful shutdown
/// coordination.
pub struct WorkerPool {
    workers: Arc<Vec<mpsc::Sender<WorkRequest>>>,
    next_worker: AtomicUsize,
    shutdown_token: CancellationToken,
}

impl WorkerPool {
    /// Constructs a new [`WorkerPool`] with a given set of channels and a
    /// shared shutdown token.
    pub fn new(
        workers: Arc<Vec<mpsc::Sender<WorkRequest>>>,
        shutdown_token: CancellationToken,
    ) -> Self {
        Self {
            workers,
            next_worker: AtomicUsize::new(0),
            shutdown_token,
        }
    }

    /// Returns the next worker index using a relaxed atomic round-robin
    /// counter.
    pub(crate) fn next_worker_index(&self) -> usize {
        self.next_worker.fetch_add(1, Ordering::Relaxed) % self.workers.len()
    }

    /// Sends a [`WorkRequest`] to the next available worker.
    ///
    /// Uses `try_send()` for fast-path success, falling back to a bounded
    /// `send()` call with a timeout (100ms). If the shutdown token is already
    /// cancelled, returns [`IdServiceError::ServiceShutdown`].
    pub(crate) async fn send_to_next_worker(
        &self,
        request: WorkRequest,
    ) -> Result<(), IdServiceError> {
        if self.shutdown_token.is_cancelled() {
            return Err(IdServiceError::ServiceShutdown);
        }

        let worker_idx = self.next_worker_index();
        let worker = &self.workers[worker_idx];

        match worker.try_send(request) {
            Ok(()) => Ok(()),
            Err(mpsc::error::TrySendError::Full(request)) => {
                match tokio::time::timeout(
                    tokio::time::Duration::from_millis(100),
                    worker.send(request),
                )
                .await
                {
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

    /// Initiates cooperative shutdown of all workers in the pool.
    ///
    /// Broadcasts a shutdown signal via the shared [`CancellationToken`] and
    /// sends a [`WorkRequest::Shutdown`] to each worker. Waits for a response
    /// from each worker (with a timeout of 5 seconds).
    pub(crate) async fn shutdown(&self) -> Result<(), IdServiceError> {
        #[cfg(feature = "tracing")]
        tracing::info!("Initiating worker pool shutdown");

        // Stop producing more work from existing streams
        self.shutdown_token.cancel();

        let mut shutdown_handles = Vec::new();

        for (_i, worker) in self.workers.iter().enumerate() {
            let (tx, rx) = oneshot::channel();
            if let Err(_e) = worker.send(WorkRequest::Shutdown { response: tx }).await {
                // typically: "channel closed"
                #[cfg(feature = "tracing")]
                tracing::debug!("Failed to send shutdown signal to worker {}: {}", _i, _e);
            } else {
                shutdown_handles.push(rx);
            }
        }

        for (_i, handle) in shutdown_handles.into_iter().enumerate() {
            match tokio::time::timeout(tokio::time::Duration::from_secs(3), handle).await {
                Ok(Ok(())) => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} shut down gracefully", _i)
                }
                Ok(Err(_e)) => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} shutdown response: {}", _i, _e)
                }
                Err(_) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("Worker {} shutdown timeout", _i)
                }
            }
        }

        #[cfg(feature = "tracing")]
        tracing::info!("Worker pool shutdown complete");
        Ok(())
    }
}
