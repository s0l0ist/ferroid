use crate::server::{
    config::ServerConfig,
    service::config::SnowflakeGeneratorType,
    streaming::{processor::handle_stream_request, request::WorkRequest},
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
pub async fn worker_loop(
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
