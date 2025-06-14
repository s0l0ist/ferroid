//! # A gRPC Server for Streaming Snowflake ID Generation
//!
//! `ferroid-tonic-server` is a high-performance, streaming gRPC server for
//! batch Snowflake-style ID generation, built with [`tonic`] and powered by
//! [`ferroid`](https://github.com/s0l0ist/ferroid).
//!
//! This server is optimized for latency-sensitive and high-throughput workloads
//! - such as distributed queues, event ingestion pipelines, or scalable
//! database key generation-where time-ordered, collision-free IDs are critical.
//!
//! ## Features
//!
//! - **Streaming gRPC Interface**: Clients request batches of IDs via the
//!   `StreamIds` endpoint; IDs are streamed back in chunks (compression
//!   optional).
//! - **Zstd, Gzip, Deflate Compression**: Negotiated via gRPC per stream.
//! - **Per-Worker ID Generators**: Each async task owns its own generator and
//!   shard (Snowflake `worker_id`), ensuring scale-out safety and eliminating
//!   contention.
//! - **Backpressure-aware**: Bounded queues prevent unbounded memory growth.
//! - **Graceful Shutdown**: Ensures all in-flight work completes.
//! - **Client Cancellation**: Stream requests are interruptible mid-flight.
//!
//! ## Running the Server
//!
//! Install:
//!
//! ```bash
//! # Install with the `tracing` feature to see traces/logs adjustable with RUST_LOG
//! cargo install ferroid-tonic-server --features tracing
//!
//! # If you want full telemetry, it supports exporting to Honeycomb
//! cargo install ferroid-tonic-server --features tracing,metrics,honeycomb
//! ```
//!
//! Run with a specific number of workers. Each worker corresponds to the
//! `machine_id` of the Snowflake ID:
//! ```bash
//! ./ferroid-tonic-server --num-workers 16
//! ```
//!
//! The server listens on `0.0.0.0:50051` by default. You can override the
//! address via CLI or environment variables (see `--help`).
//!
//! ## Example: List Services via Reflection
//!
//! ```bash
//! grpcurl -plaintext localhost:50051 list
//! > ferroid.IdGenerator
//! > grpc.reflection.v1.ServerReflection
//! ```
//!
//! You can run an example query, but the results are in base64 binary packed
//! form from grpcurl. To deserialize properly, checkout the benchmarks:
//!
//! ```bash
//! grpcurl -plaintext \                   
//!   -d '{"count": 1}' \
//!   localhost:50051 \
//!   ferroid.IdGenerator/StreamIds
//!
//! {
//!   "packedIds": "AADANUc+1AA="
//! }
//! ```
//!
//! ## Healthcheck
//!
//! ```bash
//! ./grpc-health-probe -addr=localhost:50051 -service=ferroid.IdGenerator
//! > status: SERVING
//! ```
//!
//! ## Integration
//!
//! - Import the `.proto` file from `ferroid-tonic`
//! - Use `IdGenerator.StreamIds` for streaming ID allocation
//! - Each response chunk (`IdChunk`) contains a packed byte buffer of IDs
//!
//! ### Notes
//!
//! - ID size (e.g., `u64`, `u128`) must be inferred by the client
//! - IDs are packed in little-endian binary format (see `IdChunk.packed_ids`)

mod server;

use clap::Parser;
use ferroid_tonic_core::proto::{FILE_DESCRIPTOR_SET, id_generator_server::IdGeneratorServer};
use futures::Stream;
use server::config::{CliArgs, ServerConfig};
use server::service::handler::IdService;
use server::telemetry::{TelemetryProviders, init_telemetry};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio::signal;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::transport::server::Connected;
use tonic::{codec::CompressionEncoding, transport::Server};
use tonic_health::server::HealthReporter;
use tonic_reflection::server::Builder;
use tonic_web::GrpcWebLayer;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};

// Using mimalloc for better performance under contention, especially in musl
// environments.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load from .env
    let _ = dotenvy::dotenv();
    let args = CliArgs::parse();
    let config = ServerConfig::try_from(args)?;

    let providers = init_telemetry()?;

    if config.uds {
        #[cfg(unix)]
        {
            use tokio::net::UnixListener;
            use tokio_stream::wrappers::UnixListenerStream;
            let uds_path = config.server_addr.clone();
            let uds = UnixListener::bind(&uds_path)?;
            let incoming = UnixListenerStream::new(uds);
            log_startup_info(&uds_path, &config);
            let res = run_server_with_incoming(providers, incoming, config).await;
            // TODO: Best effort to clean up the socket file although a panic
            // might leave it behind.
            let _ = std::fs::remove_file(&uds_path);
            res
        }
        #[cfg(not(unix))]
        {
            anyhow::bail!("Unix domain sockets are not supported on this platform");
        }
    } else {
        let tcp_path = config.server_addr.clone();
        let tcp = TcpListener::bind(&tcp_path).await?;
        let incoming = TcpListenerStream::new(tcp);
        log_startup_info(&tcp_path, &config);
        run_server_with_incoming(providers, incoming, config).await
    }
}

async fn run_server_with_incoming<I, IO, IE>(
    providers: TelemetryProviders,
    incoming: I,
    config: ServerConfig,
) -> anyhow::Result<()>
where
    I: Stream<Item = Result<IO, IE>>,
    IO: AsyncRead + AsyncWrite + Connected + Unpin + Send + 'static,
    IE: Into<tower::BoxError>,
{
    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<IdGeneratorServer<IdService>>()
        .await;

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
        .add_service(health_service.clone())
        .add_service(reflection)
        .add_service(build_id_service(service.clone()))
        .serve_with_incoming_shutdown(
            incoming,
            shutdown_signal(service, health_reporter, providers),
        )
        .await?;

    #[cfg(feature = "tracing")]
    tracing::info!("Service shut down successfully");
    Ok(())
}

fn log_startup_info(_addr: &str, _config: &ServerConfig) {
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
fn build_id_service(service: IdService) -> IdGeneratorServer<IdService> {
    IdGeneratorServer::new(service)
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
        .set_not_serving::<IdGeneratorServer<IdService>>()
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
