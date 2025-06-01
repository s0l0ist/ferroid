//! Worker task lifecycle and stream request execution for Snowflake ID
//! generation.
//!
//! This module defines the logic executed by each background worker responsible
//! for processing [`WorkRequest`] messages. A worker is an independent async
//! task that listens on a bounded channel for incoming commands, which may
//! include:
//!
//! - Streamed ID generation requests (`WorkRequest::Stream`)
//! - Graceful shutdown signals (`WorkRequest::Shutdown`)
//!
//! Each worker owns a dedicated [`SnowflakeGeneratorType`] instance, ensuring
//! uniqueness in ID generation across the pool. Chunked ID generation is
//! delegated to [`handle_stream_request`], which handles buffering,
//! backpressure, and cancellation detection.
//!
//! ## Responsibilities
//!
//! - Maintain a long-running loop that receives and processes work.
//! - Cooperate with cancellation via [`CancellationToken`].
//! - Ensure clean shutdown by responding to shutdown signals.
//!
//! This module is used internally by the worker pool and should not be invoked
//! directly outside of pool orchestration.

use crate::server::{
    config::ServerConfig,
    service::{
        config::SnowflakeGeneratorType,
        streaming::{processor::handle_stream_request, request::WorkRequest},
    },
};
use tokio::sync::mpsc;

/// Main execution loop for a worker task.
///
/// This function listens for [`WorkRequest`] messages on the provided channel.
/// For each `Stream` request, it invokes the chunked ID generation pipeline.
/// For `Shutdown`, it exits cleanly after acknowledging shutdown.
///
/// Each worker owns its own [`SnowflakeGeneratorType`] to maintain ID
/// uniqueness.
///
/// # Arguments
///
/// - `worker_id`: Unique numeric ID of the worker, used for logging and
///   tracing
/// - `rx`: Receiver for incoming [`WorkRequest`] messages.
/// - `generator`: The Snowflake ID generator owned by this worker.
///
/// This function is intended to be spawned as a Tokio task and runs until a
/// shutdown signal is received.
pub(crate) async fn worker_loop(
    worker_id: usize,
    mut rx: mpsc::Receiver<WorkRequest>,
    mut generator: SnowflakeGeneratorType,
    config: ServerConfig,
) {
    #[cfg(feature = "tracing")]
    tracing::debug!("Worker {} started", worker_id);

    loop {
        while let Some(work) = rx.recv().await {
            match work {
                WorkRequest::Stream {
                    count,
                    tx,
                    cancelled,
                } => {
                    handle_stream_request(worker_id, count, tx, cancelled, &mut generator, &config)
                        .await;
                }
                WorkRequest::Shutdown { response } => {
                    #[cfg(feature = "tracing")]
                    tracing::debug!("Worker {} received shutdown signal", worker_id);
                    let _ = response.send(());
                    break;
                }
            }
        }

        #[cfg(feature = "tracing")]
        tracing::debug!("Worker {} stopped", worker_id);
    }
}
