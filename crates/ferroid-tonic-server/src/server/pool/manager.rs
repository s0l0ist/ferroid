//! Asynchronous worker pool for chunked ID generation.
//!
//! This module defines the [`WorkerPool`] struct, which manages a set of
//! asynchronous workers responsible for processing [`WorkRequest`]s. It
//! distributes work using round-robin scheduling and supports coordinated
//! shutdown via a shared [`CancellationToken`].
//!
//! Each worker listens on its own bounded [`mpsc::Receiver`] and executes tasks
//! independently. This model allows efficient parallelism without contention or
//! locking.

use crate::server::{
    service::handler::{get_streams_inflight, set_global_shutdown},
    streaming::request::WorkRequest,
};
use core::time::Duration;
use ferroid_tonic_core::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::{
    sync::{mpsc, oneshot},
    time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;

/// A cooperative pool of asynchronous workers that process [`WorkRequest`]s.
///
/// Workers receive requests over bounded MPSC channels. Work is distributed in
/// round-robin fashion and the pool supports graceful, cancellable shutdown.
pub struct WorkerPool {
    workers: Vec<mpsc::Sender<WorkRequest>>,
    next_worker: AtomicUsize,
    shutdown_token: CancellationToken,
    shutdown_timeout: usize,
}

impl WorkerPool {
    /// Constructs a new [`WorkerPool`] from initialized worker channels and a
    /// shared cancellation token.
    pub const fn new(
        workers: Vec<mpsc::Sender<WorkRequest>>,
        shutdown_token: CancellationToken,
        shutdown_timeout: usize,
    ) -> Self {
        Self {
            workers,
            next_worker: AtomicUsize::new(0),
            shutdown_token,
            shutdown_timeout,
        }
    }

    /// Returns the index of the next worker to receive work (round-robin).
    ///
    /// Uses a relaxed atomic increment to minimize contention.
    pub fn next_worker_index(&self) -> usize {
        self.next_worker.fetch_add(1, Ordering::Relaxed) % self.workers.len()
    }

    /// Sends a [`WorkRequest`] to the next available worker in the pool.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The service is shutting down (`shutdown_token` was cancelled).
    /// - The worker's channel is closed.
    pub async fn send_to_next_worker(&self, request: WorkRequest) -> Result<(), Error> {
        if self.shutdown_token.is_cancelled() {
            return Err(Error::ServiceShutdown);
        }

        let worker_idx = self.next_worker_index();
        let worker = &self.workers[worker_idx];

        match worker.send(request).await {
            Ok(()) => Ok(()),
            Err(_) => Err(Error::ChannelError {
                context: format!("Worker {worker_idx} channel closed"),
            }),
        }
    }

    /// Gracefully shuts down all workers in the pool.
    ///
    /// - Cancels the shared [`CancellationToken`] to prevent new work.
    /// - Sends a [`WorkRequest::Shutdown`] to each worker.
    /// - Waits (up to 3 seconds per worker) for shutdown acknowledgements.
    ///
    /// This method is typically invoked during service termination.
    pub async fn shutdown(&self) -> Result<(), Error> {
        // === Phase 0: Stop accepting new requests ===
        #[cfg(feature = "tracing")]
        tracing::info!("Refusing new requests");
        set_global_shutdown();

        // === Phase 1: Wait for in-flight streams to drain (up to 3s) ===
        #[cfg(feature = "tracing")]
        tracing::info!(
            "Draining in-flight streams ({} active)",
            get_streams_inflight()
        );
        let drain_result = timeout(Duration::from_secs(self.shutdown_timeout as u64), async {
            while get_streams_inflight() > 0 {
                sleep(Duration::from_millis(100)).await;
            }
        })
        .await;

        match drain_result {
            Ok(()) => {
                #[cfg(feature = "tracing")]
                tracing::debug!("All in-flight streams drained successfully");
            }
            Err(_) => {
                #[cfg(feature = "tracing")]
                tracing::warn!(
                    "Graceful drain timed out ({} streams still active)",
                    get_streams_inflight()
                );
            }
        }

        // === Phase 2: Cancel any remaining work ===
        #[cfg(feature = "tracing")]
        tracing::debug!("Cancelling remaining work via shutdown token");
        self.shutdown_token.cancel();

        // === Phase 3: Notify workers to shut down ===
        #[cfg(feature = "tracing")]
        tracing::debug!("Notifying all workers to shut down");
        let mut shutdown_handles = Vec::with_capacity(self.workers.len());

        for (i, worker) in self.workers.iter().enumerate() {
            let (tx, rx) = oneshot::channel();
            if let Err(_e) = worker.send(WorkRequest::Shutdown { response: tx }).await {
                #[cfg(feature = "tracing")]
                tracing::error!("Failed to send shutdown to worker {i}: {_e}");
            } else {
                shutdown_handles.push((i, rx));
            }
        }

        #[cfg(feature = "tracing")]
        tracing::debug!("Waiting for up to 3s per worker for shutdown acknowledgements");

        let timeout_futures = shutdown_handles.into_iter().map(|(_i, rx)| async move {
            match timeout(Duration::from_secs(3), rx).await {
                Ok(Ok(())) => {
                    #[cfg(feature = "tracing")]
                    tracing::trace!("Worker {_i} shutdown acknowledged");
                }
                Ok(Err(_e)) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!("Worker {_i} returned error: {_e}");
                }
                Err(_) => {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("Worker {_i} shutdown timed out");
                }
            }
        });

        futures::future::join_all(timeout_futures).await;

        #[cfg(feature = "tracing")]
        tracing::info!("Worker pool shutdown complete");

        Ok(())
    }
}
