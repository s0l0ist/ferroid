//! # Telemetry Features
//!
//! This crate supports optional telemetry using the `tracing` and `metrics`
//! crates, exported via OpenTelemetry to either Honeycomb or stdout.
//!
//! ## Feature matrix
//!
//! - `tracing`: Enables OpenTelemetry distributed tracing (via spans).
//! - `metrics`: Enables OpenTelemetry metrics (via counters, histograms, etc.).
//! - `honeycomb`: Enables the Honeycomb OTLP exporter.
//! - `stdout`: Enables the stdout OTLP exporter.
//!
//! ## Feature constraints
//!
//! - Exporters require using at least one of: `tracing` or `metrics`.
//! - Both `honeycomb` and `stdout` exporters can be enabled at the same time.
//!
//! ## Span behavior
//!
//! - Spans created via `tracing::info_span!` are exported to any enabled
//!   telemetry backend
//! - Events (`tracing::info!`, etc.) inside a span become span events in
//!   telemetry backends
//! - Events outside of a span are only shown in log output (via
//!   `fmt::layer()`), not exported
//!
//! ## Metrics behavior
//!
//! - Metrics (e.g. request count, stream duration) are exported if `metrics` is
//!   enabled
//! - Each exporter (Honeycomb, stdout) gets its own reader
//!
//! ## Example usage
//!
//! Enable tracing and export to Honeycomb:
//!
//! ```bash
//! cargo run --features tracing,honeycomb
//! ```
//!
//! Enable metrics and export to Honeycomb:
//!
//! ```bash
//! cargo run--features metrics,honeycomb
//! ```
//!
//! Enable tracing and metrics, exported to both Honeycomb and stdout:
//!
//! ```bash
//! cargo run --features tracing,metrics,honeycomb,stdout
//! ```
//!
//! Enable only local stdout export (no remote backend):
//!
//! ```bash
//! cargo run --features tracing,stdout
//! ```

// Disallow using `honeycomb` without `tracing` or `metrics`
#[cfg(all(
    feature = "honeycomb",
    not(any(feature = "tracing", feature = "metrics"))
))]
compile_error!(
    "The 'honeycomb' feature requires at least one of 'tracing' or 'metrics' to be enabled."
);

// Disallow using `stdout` without `tracing` or `metrics`
#[cfg(all(feature = "stdout", not(any(feature = "tracing", feature = "metrics"))))]
compile_error!(
    "The 'stdout' feature requires at least one of 'tracing' or 'metrics' to be enabled."
);

// Core imports - always needed
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

// Honeycomb-specific imports
#[cfg(all(feature = "honeycomb", any(feature = "metrics", feature = "tracing")))]
use opentelemetry_otlp::{Compression, Protocol, WithExportConfig, WithTonicConfig};
#[cfg(all(feature = "honeycomb", feature = "metrics"))]
use opentelemetry_sdk::metrics::Temporality;
#[cfg(feature = "honeycomb")]
use tonic::metadata::MetadataMap;
#[cfg(all(feature = "honeycomb", any(feature = "metrics", feature = "tracing")))]
use tonic::transport::ClientTlsConfig;

// Metrics-specific imports
#[cfg(feature = "metrics")]
use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};
#[cfg(feature = "metrics")]
use opentelemetry_sdk::metrics as sdkmetrics;
#[cfg(feature = "metrics")]
use std::sync::OnceLock;

// Either
#[cfg(any(feature = "metrics", feature = "tracing"))]
use opentelemetry::{InstrumentationScope, KeyValue};
#[cfg(any(feature = "metrics", feature = "tracing"))]
use opentelemetry_sdk::Resource;
#[cfg(any(feature = "metrics", feature = "tracing"))]
use opentelemetry_semantic_conventions as semvcns;

// Tracing-specific imports
#[cfg(feature = "tracing")]
use opentelemetry::trace::TracerProvider;
#[cfg(feature = "tracing")]
use opentelemetry_sdk::propagation::TraceContextPropagator;
#[cfg(feature = "tracing")]
use opentelemetry_sdk::trace as sdktrace;

pub struct TelemetryProviders {
    #[cfg(feature = "tracing")]
    pub tracer_provider: sdktrace::SdkTracerProvider,
    #[cfg(feature = "metrics")]
    pub meter_provider: sdkmetrics::SdkMeterProvider,
}

pub fn init_telemetry() -> anyhow::Result<TelemetryProviders> {
    #[cfg(feature = "tracing")]
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    #[cfg(feature = "tracing")]
    let tracer_provider = init_tracer()?;

    #[cfg(feature = "metrics")]
    let meter_provider = init_metrics()?;

    #[cfg(any(feature = "metrics", feature = "tracing"))]
    let scope = InstrumentationScope::builder("ferroid")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_schema_url(semvcns::SCHEMA_URL)
        .build();

    // Always subscribe to standard tracing logs printed to the console via
    // `tracing_subscriber::fmt`. This is unrelated to the `opentelemetry_stdout`
    // exporter - it logs spans/events as human-readable output.
    let registry = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(
            tracing_subscriber::fmt::layer()
                .with_thread_ids(true)
                .with_line_number(true)
                .with_target(false)
                .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
                .with_file(true)
                .pretty(),
        );

    #[cfg(feature = "tracing")]
    let registry = {
        opentelemetry::global::set_tracer_provider(tracer_provider.clone());
        registry.with(
            tracing_opentelemetry::layer()
                .with_tracer(tracer_provider.tracer_with_scope(scope.clone()))
                .with_error_records_to_exceptions(true),
        )
    };

    #[cfg(feature = "metrics")]
    let registry = {
        opentelemetry::global::set_meter_provider(meter_provider.clone());
        let meter = opentelemetry::global::meter_with_scope(scope);
        init_metric_handles(meter);

        registry.with(tracing_opentelemetry::MetricsLayer::new(
            meter_provider.clone(),
        ))
    };

    registry.init();

    Ok(TelemetryProviders {
        #[cfg(feature = "tracing")]
        tracer_provider,
        #[cfg(feature = "metrics")]
        meter_provider,
    })
}

#[cfg(feature = "honeycomb")]
fn get_metadata() -> anyhow::Result<MetadataMap> {
    use anyhow::Context;

    let mut map = MetadataMap::new();
    let api_key = std::env::var("HONEYCOMB_API_KEY").context("missing `HONEYCOMB_API_KEY`")?;
    let dataset = std::env::var("HONEYCOMB_DATASET").context("missing `HONEYCOMB_DATASET`")?;
    map.insert(
        "x-honeycomb-team",
        api_key.parse().context("invalid API key")?,
    );
    map.insert(
        "x-honeycomb-dataset",
        dataset.parse().context("invalid dataset")?,
    );
    Ok(map)
}

#[cfg(any(feature = "metrics", feature = "tracing"))]
fn resource() -> Resource {
    Resource::builder()
        .with_service_name("ferroid")
        .with_schema_url(
            [KeyValue::new(
                semvcns::resource::SERVICE_VERSION,
                env!("CARGO_PKG_VERSION"),
            )],
            semvcns::SCHEMA_URL,
        )
        .build()
}

#[cfg(feature = "metrics")]
fn init_metrics() -> anyhow::Result<sdkmetrics::SdkMeterProvider> {
    let builder = sdkmetrics::SdkMeterProvider::builder().with_resource(resource());

    #[cfg(feature = "stdout")]
    let builder = {
        use opentelemetry_stdout::MetricExporter;
        let exporter = MetricExporter::default();
        let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(exporter)
            .with_interval(std::time::Duration::from_secs(5))
            .build();

        builder.with_reader(reader)
    };

    #[cfg(feature = "honeycomb")]
    let builder = {
        use anyhow::Context;

        let metadata = get_metadata()?;
        let endpoint =
            std::env::var("HONEYCOMB_ENDPOINT").context("missing `HONEYCOMB_API_KEY`")?;
        let compression = {
            use std::str::FromStr;
            let raw = std::env::var("HONEYCOMB_COMPRESSION")
                .context("missing `HONEYCOMB_API_KEY`")?
                .to_ascii_lowercase();
            Compression::from_str(&raw)?
        };
        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_tls_config(ClientTlsConfig::new().with_native_roots())
            .with_metadata(metadata)
            .with_timeout(std::time::Duration::from_secs(10))
            .with_compression(compression)
            .with_endpoint(endpoint)
            .with_protocol(Protocol::Grpc)
            .with_temporality(Temporality::Delta)
            .build()
            .context("failed to build metrics exporter")?;

        builder.with_periodic_exporter(exporter)
    };

    Ok(builder.build())
}

#[cfg(feature = "tracing")]
fn init_tracer() -> anyhow::Result<sdktrace::SdkTracerProvider> {
    let builder = sdktrace::SdkTracerProvider::builder().with_resource(resource());

    #[cfg(feature = "stdout")]
    let builder = {
        use opentelemetry_stdout::SpanExporter;
        let exporter = SpanExporter::default();
        let batch = sdktrace::BatchSpanProcessor::builder(exporter)
            .with_batch_config(
                sdktrace::BatchConfigBuilder::default()
                    .with_scheduled_delay(std::time::Duration::from_secs(5))
                    .with_max_queue_size(2048)
                    .build(),
            )
            .build();
        builder.with_span_processor(batch)
    };

    #[cfg(feature = "honeycomb")]
    let builder = {
        use anyhow::Context;

        let metadata = get_metadata()?;
        let endpoint =
            std::env::var("HONEYCOMB_ENDPOINT").context("missing `HONEYCOMB_API_KEY`")?;
        let compression = {
            use std::str::FromStr;
            let raw = std::env::var("HONEYCOMB_COMPRESSION")
                .context("missing `HONEYCOMB_API_KEY`")?
                .to_ascii_lowercase();
            Compression::from_str(&raw)?
        };
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_tls_config(ClientTlsConfig::new().with_native_roots())
            .with_metadata(metadata)
            .with_timeout(std::time::Duration::from_secs(10))
            .with_compression(compression)
            .with_endpoint(endpoint)
            .with_protocol(Protocol::Grpc)
            .build()
            .context("failed to build tracer exporter")?;

        let batch = sdktrace::BatchSpanProcessor::builder(exporter)
            .with_batch_config(
                sdktrace::BatchConfigBuilder::default()
                    .with_scheduled_delay(std::time::Duration::from_secs(5))
                    .with_max_queue_size(2048)
                    .build(),
            )
            .build();

        builder.with_span_processor(batch)
    };

    Ok(builder.build())
}

// Metric handles - only compiled when metrics feature is enabled
#[cfg(feature = "metrics")]
static REQUESTS: OnceLock<Counter<u64>> = OnceLock::new();
#[cfg(feature = "metrics")]
static STREAMS_INFLIGHT: OnceLock<UpDownCounter<i64>> = OnceLock::new();
#[cfg(feature = "metrics")]
static STREAM_ERRORS: OnceLock<Counter<u64>> = OnceLock::new();
#[cfg(feature = "metrics")]
static STREAM_DURATION_MS: OnceLock<Histogram<f64>> = OnceLock::new();
#[cfg(feature = "metrics")]
static IDS_GENERATED: OnceLock<Counter<u64>> = OnceLock::new();
#[cfg(feature = "metrics")]
static IDS_PER_REQUEST: OnceLock<Histogram<f64>> = OnceLock::new();

#[cfg(feature = "metrics")]
fn init_metric_handles(meter: Meter) {
    let _ = REQUESTS.set(
        meter
            .u64_counter("requests")
            .with_description("Total gRPC stream requests")
            .build(),
    );

    let _ = STREAMS_INFLIGHT.set(
        meter
            .i64_up_down_counter("streams_inflight")
            .with_description("Concurrent gRPC streams")
            .build(),
    );

    let _ = STREAM_ERRORS.set(
        meter
            .u64_counter("errors")
            .with_description("Errored/cancelled streams")
            .build(),
    );

    let _ = STREAM_DURATION_MS.set(
        meter
            .f64_histogram("stream_duration")
            .with_unit("ms")
            .with_description("End-to-end stream duration")
            .build(),
    );

    let _ = IDS_GENERATED.set(
        meter
            .u64_counter("ids_generated")
            .with_description("Total Snowflake IDs generated")
            .build(),
    );

    let _ = IDS_PER_REQUEST.set(
        meter
            .f64_histogram("ids_per_request")
            .with_description("IDs requested per stream")
            .build(),
    );
}

// Convenience functions that compile to no-ops when metrics are disabled
#[cfg(feature = "metrics")]
pub fn increment_requests() {
    if let Some(counter) = REQUESTS.get() {
        counter.add(1, &[]);
    }
}

#[cfg(not(feature = "metrics"))]
pub fn increment_requests() {}

#[cfg(feature = "metrics")]
pub fn increment_streams_inflight() {
    if let Some(counter) = STREAMS_INFLIGHT.get() {
        counter.add(1, &[]);
    }
}

#[cfg(not(feature = "metrics"))]
pub fn increment_streams_inflight() {}

#[cfg(feature = "metrics")]
pub fn decrement_streams_inflight() {
    if let Some(counter) = STREAMS_INFLIGHT.get() {
        counter.add(-1, &[]);
    }
}

#[cfg(not(feature = "metrics"))]
pub fn decrement_streams_inflight() {}

#[cfg(feature = "metrics")]
pub fn increment_stream_errors() {
    if let Some(counter) = STREAM_ERRORS.get() {
        counter.add(1, &[]);
    }
}

#[cfg(not(feature = "metrics"))]
pub fn increment_stream_errors() {}

#[cfg(feature = "metrics")]
pub fn record_stream_duration(duration_ms: f64) {
    if let Some(histogram) = STREAM_DURATION_MS.get() {
        histogram.record(duration_ms, &[]);
    }
}

#[cfg(not(feature = "metrics"))]
pub fn record_stream_duration(_duration_ms: f64) {}

#[cfg(feature = "metrics")]
pub fn increment_ids_generated(count: u64) {
    if let Some(counter) = IDS_GENERATED.get() {
        counter.add(count, &[]);
    }
}

#[cfg(not(feature = "metrics"))]
pub fn increment_ids_generated(_count: u64) {}

#[cfg(feature = "metrics")]
pub fn record_ids_per_request(count: f64) {
    if let Some(histogram) = IDS_PER_REQUEST.get() {
        histogram.record(count, &[]);
    }
}

#[cfg(not(feature = "metrics"))]
pub fn record_ids_per_request(_count: f64) {}
