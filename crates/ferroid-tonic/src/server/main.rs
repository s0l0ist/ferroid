//! # `ferroid-tonic`: Streaming Snowflake ID Generation Service
//!
//! `ferroid-tonic` is a high-performance, gRPC-based Snowflake ID generation
//! service built on top of [`ferroid`] for timestamp-based ID generation and
//! [`tonic`] for HTTP/2 transport.
//!
//! This crate powers a standalone server that accepts streaming requests for
//! batches of Snowflake-like IDs. It is designed for workloads that demand:
//!
//! - Time-ordered, collision-free IDs
//! - Large batch throughput
//! - Efficient use of memory and network bandwidth
//!
//! ## Highlights
//!
//! - **Streaming-only gRPC Endpoint**: Clients stream requests for up to
//!   billions of IDs per call, delivered in compressed, chunked responses.
//! - **Tokio Worker Pool**: Each worker runs a dedicated ID generator with
//!   bounded task queues.
//! - **Backpressure Aware**: All queues are size-limited to avoid memory
//!   blowup.
//! - **Client Cancellation**: Streamed requests are cancellable mid-flight.
//! - **Graceful Shutdown**: Coordinated shutdown ensures no work is lost.
//! - **Zstd Compression**: gRPC chunks are compressed for efficient transfer.
//!
//! ## Usage
//!
//! Build and run the gRPC server with:
//!
//! ```bash
//! cargo run --bin tonic-server --release
//! ```
//!
//! Then connect via gRPC on `127.0.0.1:50051` using the `GetStreamIds`
//! endpoint.
//!
//! ## Reflection
//! ```bash
//! grpcurl -plaintext localhost:50051 list
//! > grpc.reflection.v1.ServerReflection
//! > idgen.IdGen
//! ```
//!
//! ## Healthcheck
//! ```bash
//! ./grpc-health-probe -addr=localhost:50051 -service=idgen.IdGen
//! > status: SERVING
//! ```

mod config;
mod pool;
mod service;
mod streaming;
mod telemetry;

use clap::Parser;
use config::{CliArgs, ServerConfig};
use ferroid_tonic::common::idgen::{FILE_DESCRIPTOR_SET, id_gen_server::IdGenServer};
use service::handler::IdService;
use std::net::SocketAddr;
use telemetry::{TelemetryProviders, init_telemetry};
use tokio::signal;
use tonic::{codec::CompressionEncoding, transport::Server};
use tonic_health::server::HealthReporter;
use tonic_reflection::server::Builder;
use tonic_web::GrpcWebLayer;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load from .env
    dotenvy::dotenv()?;

    let args = CliArgs::try_parse()?;
    let config = ServerConfig::try_from(args)?;

    let addr: SocketAddr = "[::1]:50051".parse()?;
    run_server(addr, config).await
}

async fn run_server(addr: SocketAddr, config: ServerConfig) -> anyhow::Result<()> {
    let providers = init_telemetry()?;

    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<IdGenServer<IdService>>()
        .await;

    let service = IdService::new(config.clone());

    let reflection = Builder::configure()
        .register_encoded_file_descriptor_set(FILE_DESCRIPTOR_SET)
        .build_v1()?;

    log_startup_info(&addr, &config);

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
        .add_service(health_service.clone())
        .add_service(reflection)
        .add_service(build_id_service(service.clone()))
        .serve_with_shutdown(addr, shutdown_signal(service, health_reporter, providers))
        .await?;

    #[cfg(feature = "tracing")]
    tracing::info!("Service shut down successfully");
    Ok(())
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
fn build_id_service(service: IdService) -> IdGenServer<IdService> {
    IdGenServer::new(service)
        .send_compressed(CompressionEncoding::Zstd)
        .send_compressed(CompressionEncoding::Gzip)
        .send_compressed(CompressionEncoding::Deflate)
        .accept_compressed(CompressionEncoding::Zstd)
        .accept_compressed(CompressionEncoding::Gzip)
        .accept_compressed(CompressionEncoding::Deflate)
}

async fn shutdown_signal(
    service: IdService,
    health_reporter: HealthReporter,
    providers: TelemetryProviders,
) {
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

    health_reporter
        .set_not_serving::<IdGenServer<IdService>>()
        .await;

    if let Err(_e) = service.shutdown().await {
        #[cfg(feature = "tracing")]
        tracing::error!("Error during service shutdown: {:?}", _e);
    }

    #[cfg(feature = "tracing")]
    {
        if let Err(err) = providers.tracer_provider.force_flush() {
            eprintln!("Error flushing traces: {:#?}", err);
        }
        if let Err(err) = providers.tracer_provider.shutdown() {
            eprintln!("Error shutting down tracer: {:#?}", err);
        }
    }

    #[cfg(feature = "metrics")]
    {
        if let Err(err) = providers.meter_provider.force_flush() {
            eprintln!("Error flushing metrics: {:#?}", err);
        }
        if let Err(err) = providers.meter_provider.shutdown() {
            eprintln!("Error shutting down meter: {:#?}", err);
        }
    }

    // manually drop the provider(s) before invoking shutdown.
    drop(providers);
}
