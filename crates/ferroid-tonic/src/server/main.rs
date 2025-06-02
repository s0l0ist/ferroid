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
    common::idgen::{FILE_DESCRIPTOR_SET, id_gen_server::IdGenServer},
    server::{
        config::{CliArgs, ServerConfig},
        service::handler::IdService,
        telemetry::init_tracing,
    },
};
use std::net::SocketAddr;
use tokio::signal;
use tonic::{codec::CompressionEncoding, transport::Server};
use tonic_reflection::server::Builder;
use tonic_web::GrpcWebLayer;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};

/// Launches the gRPC streaming ID generation service.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::try_parse()?;
    let config = ServerConfig::try_from(args)?;

    init_tracing();

    let addr = "127.0.0.1:50051".parse()?;
    log_startup_info(&addr, &config);

    let service = IdService::new(config);
    let reflection_service = Builder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .build_v1()
        .unwrap();

    let server = Server::builder()
        .accept_http1(true)
        .http2_adaptive_window(Some(true))
        .layer(
            ServiceBuilder::new()
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods(Any)
                        .allow_headers(Any),
                )
                .layer(GrpcWebLayer::new()),
        )
        .add_service(reflection_service)
        .add_service(
            IdGenServer::new(service.clone())
                .send_compressed(CompressionEncoding::Zstd)
                .send_compressed(CompressionEncoding::Gzip)
                .send_compressed(CompressionEncoding::Deflate)
                .accept_compressed(CompressionEncoding::Zstd)
                .accept_compressed(CompressionEncoding::Gzip)
                .accept_compressed(CompressionEncoding::Deflate),
        )
        .serve_with_shutdown(addr, create_shutdown_signal(service));

    match server.await {
        Ok(_) => {
            println!("Service shut down successfully");
            Ok(())
        }
        Err(e) => {
            eprintln!("Server error: {:?}", e);
            Err(e.into())
        }
    }
}

fn log_startup_info(addr: &SocketAddr, config: &ServerConfig) {
    if cfg!(debug_assertions) {
        println!(
            "Starting ID service on {} with full config: {:#?}",
            addr, config
        );
    } else {
        println!(
            "Starting ID service on {} with {} workers",
            addr, config.num_workers
        );
    }
}

async fn create_shutdown_signal(service: IdService) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => println!("Received Ctrl+C signal"),
        () = terminate => println!("Received SIGTERM signal"),
    }

    println!("Shutdown signal received, terminating gracefully...");

    if let Err(e) = service.shutdown().await {
        eprintln!("Error during service shutdown: {:?}", e);
    }
}
