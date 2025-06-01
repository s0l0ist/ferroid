//! # ferroid-tonic Server
//!
//! This binary launches a high-performance, gRPC-based Snowflake ID generation
//! service using [`ferroid`] for ID generation and [`tonic`] for gRPC
//! infrastructure.
//!
//! The server provides single and streaming endpoints for clients to request
//! batches of Snowflake-like unique IDs. It is designed to handle massive
//! throughput with backpressure, cancellation, and compression support.
//!
//! ## Features
//!
//! - Streamed or single-response ID generation.
//! - Fully asynchronous, backpressure-aware worker pool.
//! - Graceful shutdown with signal handling.
//! - Adaptive HTTP/2 flow control for high-throughput gRPC traffic.
//! - Compressed responses with Zstd support.
//!
//! ## Example
//!
//! ```bash
//! cargo run --bin server --release
//! ```

use ferroid_tonic::{
    idgen::id_gen_server::IdGenServer,
    server::{config::NUM_WORKERS, service::IdService, telemetry::init_tracing},
};
use tonic::{codec::CompressionEncoding, transport::Server};

/// Entry point for the ID generation server.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();

    let addr = "127.0.0.1:50051".parse()?;
    println!(
        "Starting ID service on {} with {} workers (chunk = {}, buffer = {}, max = {})",
        addr,
        NUM_WORKERS,
        ferroid_tonic::server::config::DEFAULT_IDS_PER_CHUNK,
        ferroid_tonic::server::config::DEFAULT_WORK_REQUEST_BUFFER_SIZE,
        ferroid_tonic::server::config::MAX_ALLOWED_IDS,
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
            println!("Shutdown signal received, terminating gracefully...");

            if let Err(_e) = service_for_shutdown.shutdown().await {
                #[cfg(feature = "tracing")]
                tracing::error!("Error during service shutdown: {:?}", _e);
            }
        });

    if let Err(e) = server.await {
        #[cfg(feature = "tracing")]
        tracing::error!("Server error: {:?}", e);
        return Err(e.into());
    }

    println!("Service shut down successfully");
    Ok(())
}
