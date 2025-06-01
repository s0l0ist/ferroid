//! # `ferroid-tonic` Server
//!
//! This binary launches a high-throughput, gRPC-based Snowflake ID generation
//! service using [`ferroid`] for ID generation and [`tonic`] for the transport
//! layer.
//!
//! The server exposes a **streaming-only** endpoint that allows clients to
//! request a large number of Snowflake-like IDs, returned in compressed,
//! backpressure-aware chunks.
//!
//! ## Key Features
//!
//! - **Streaming ID Generation Only**: Optimized for large-batch use cases.
//! - **Async Worker Pool**: Fixed number of concurrent workers with unique
//!   generator state.
//! - **Backpressure Handling**: All channels are bounded to prevent memory
//!   overcommitment.
//! - **Cooperative Cancellation**: Streams terminate early when the client
//!   disconnects.
//! - **Graceful Shutdown**: Workers are shut down cleanly on `SIGINT` (Ctrl+C).
//! - **Efficient Transport**:
//!   - gRPC over HTTP/2 with adaptive windowing.
//!   - Zstd compression support for streaming responses.
//!
//! ## Running the Server
//!
//! ```bash
//! cargo run --bin server --release
//! ```
//!

use clap::Parser;
use ferroid_tonic::{
    idgen::id_gen_server::IdGenServer,
    server::{
        config::{CliArgs, ServerConfig},
        service::handler::IdService,
        telemetry::init_tracing,
    },
};
use tonic::{codec::CompressionEncoding, transport::Server};

/// Launches the gRPC streaming ID generation service.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::try_parse()?;
    let config = ServerConfig::try_from(args)?;

    init_tracing();

    let addr = "127.0.0.1:50051".parse()?;
    println!(
        "Starting ID service on {} with {} workers (chunk = {}, buffer = {}, max = {})",
        addr,
        config.num_workers,
        config.ids_per_chunk,
        config.work_request_buffer_size,
        config.max_allowed_ids,
    );

    let service = IdService::new(config);
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
