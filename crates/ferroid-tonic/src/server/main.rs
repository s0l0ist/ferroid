//! # `ferroid-tonic` gRPC Server
//!
//! High-throughput Snowflake ID service using [`ferroid`] + [`tonic`].
//!
//! ## Highlights
//! - **Streaming only**: Optimized for large batch ID generation.
//! - **Async worker pool**: Fixed concurrency, backpressure-aware.
//! - **Efficient transport**: HTTP/2, Zstd, and Gzip support.
//! - **Graceful shutdown**: Clean Ctrl+C or SIGTERM handling.
//!
//! ## Usage
//!
//! ```bash
//! cargo run --bin server --release
//! ```

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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = CliArgs::try_parse()?;
    let config = ServerConfig::try_from(args)?;

    init_tracing();

    let addr: SocketAddr = "127.0.0.1:50051".parse()?;
    log_startup_info(&addr, &config);
    run_server(addr, config).await
}

fn log_startup_info(_addr: &SocketAddr, _config: &ServerConfig) {
    if cfg!(debug_assertions) {
        #[cfg(feature = "tracing")]
        tracing::info!(
            "Starting ID service on {} with full config: {:#?}",
            _addr,
            _config
        );
    } else {
        #[cfg(feature = "tracing")]
        tracing::info!(
            "Starting ID service on {} with {} workers",
            _addr,
            _config.num_workers
        );
    }
}

async fn run_server(addr: SocketAddr, config: ServerConfig) -> anyhow::Result<()> {
    let service = IdService::new(config.clone());

    let reflection = Builder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .build_v1()?;

    Server::builder()
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
        .add_service(reflection)
        .add_service(build_id_service(service.clone()))
        .serve_with_shutdown(addr, shutdown_signal(service))
        .await?;

    #[cfg(feature = "tracing")]
    tracing::info!("Service shut down successfully");
    Ok(())
}

fn build_id_service(service: IdService) -> IdGenServer<IdService> {
    IdGenServer::new(service)
        .send_compressed(CompressionEncoding::Zstd)
        .send_compressed(CompressionEncoding::Gzip)
        .send_compressed(CompressionEncoding::Deflate)
        .accept_compressed(CompressionEncoding::Zstd)
        .accept_compressed(CompressionEncoding::Gzip)
        .accept_compressed(CompressionEncoding::Deflate)
}

async fn shutdown_signal(service: IdService) {
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    tokio::select! {
        () = ctrl_c => {
            #[cfg(feature = "tracing")]
            tracing::info!("Received Ctrl+C signal");
        },
        () = terminate => {
            #[cfg(feature = "tracing")]
            tracing::info!("Received SIGTERM signal");
        },
    }

    #[cfg(feature = "tracing")]
    tracing::info!("Shutdown signal received, terminating gracefully...");

    if let Err(_e) = service.shutdown().await {
        #[cfg(feature = "tracing")]
        tracing::error!("Error during service shutdown: {:?}", _e);
    }
}
