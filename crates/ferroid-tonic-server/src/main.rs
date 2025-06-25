#![doc = include_str!("../README.md")]

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

    // 1. Publish the status
    health_reporter
        .set_not_serving::<IdGeneratorServer<IdService>>()
        .await;

    // 2. Perform graceful shutdown
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
}
