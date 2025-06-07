use crate::{common::error::IdServiceError, server::streaming::request::WorkRequest};
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// Manages a pool of asynchronous workers for handling [`WorkRequest`]s.
///
/// Each worker operates independently, listening for incoming messages on a
/// bounded MPSC channel. The pool distributes requests using round-robin
/// scheduling and supports graceful, cooperative shutdown.
pub struct WorkerPool {
    workers: Vec<mpsc::Sender<WorkRequest>>,
    next_worker: AtomicUsize,
    shutdown_token: CancellationToken,
}

impl WorkerPool {
    /// Creates a new [`WorkerPool`] from a pre-initialized list of worker
    /// channels and a shared shutdown token.
    pub fn new(workers: Vec<mpsc::Sender<WorkRequest>>, shutdown_token: CancellationToken) -> Self {
        Self {
            workers,
            next_worker: AtomicUsize::new(0),
            shutdown_token,
        }
    }

    /// Computes the next worker index using relaxed atomic round-robin logic.
    pub fn next_worker_index(&self) -> usize {
        self.next_worker.fetch_add(1, Ordering::Relaxed) % self.workers.len()
    }

    /// Attempts to send a [`WorkRequest`] to the next worker in the pool.
    ///
    /// Returns an error if:
    /// - A server shutdown is triggered (shutdown_token)
    /// - The worker's channel is closed.
    pub async fn send_to_next_worker(&self, request: WorkRequest) -> Result<(), IdServiceError> {
        if self.shutdown_token.is_cancelled() {
            return Err(IdServiceError::ServiceShutdown);
        }

        let worker_idx = self.next_worker_index();
        let worker = &self.workers[worker_idx];

        match worker.send(request).await {
            Ok(()) => Ok(()),
            Err(_e) => Err(IdServiceError::ChannelError {
                context: format!("Worker {} channel closed", worker_idx),
            }),
        }
    }

    /// Broadcasts a shutdown signal to all workers and waits for
    /// acknowledgement.
    ///
    /// - Cancels the shared [`CancellationToken`] to prevent further work
    ///   dispatch.
    /// - Sends a [`WorkRequest::Shutdown`] to each worker, along with a
    ///   one-shot response channel.
    /// - Waits up to 3 seconds per worker for confirmation.
    ///
    /// This is typically invoked during service shutdown.
    pub async fn shutdown(&self) -> Result<(), IdServiceError> {
        #[cfg(feature = "tracing")]
        tracing::info!("Initiating worker pool shutdown");

        self.shutdown_token.cancel();
        let mut shutdown_handles = Vec::new();

        for (_i, worker) in self.workers.iter().enumerate() {
            let (tx, rx) = oneshot::channel();
            if let Err(_e) = worker.send(WorkRequest::Shutdown { response: tx }).await {
                #[cfg(feature = "tracing")]
                tracing::error!("Failed to send shutdown signal to worker {}: {}", _i, _e);
            } else {
                shutdown_handles.push(rx);
            }
        }

        let timeout_futures: Vec<_> = shutdown_handles
            .into_iter()
            .enumerate()
            .map(|(_i, handle)| async move {
                match tokio::time::timeout(tokio::time::Duration::from_secs(3), handle).await {
                    Ok(Ok(())) => {
                        #[cfg(feature = "tracing")]
                        tracing::debug!("Worker {} acked shutdown gracefully", _i);
                    }
                    Ok(Err(_e)) => {
                        #[cfg(feature = "tracing")]
                        tracing::error!("Worker {} shutdown response: {}", _i, _e);
                    }
                    Err(_) => {
                        #[cfg(feature = "tracing")]
                        tracing::warn!("Worker {} shutdown timeout", _i);
                    }
                }
            })
            .collect();

        futures::future::join_all(timeout_futures).await;

        #[cfg(feature = "tracing")]
        tracing::info!("Worker pool shutdown complete");
        Ok(())
    }
}
