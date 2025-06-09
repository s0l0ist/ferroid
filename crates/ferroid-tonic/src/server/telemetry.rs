use anyhow::Context;
use opentelemetry::metrics::{Counter, Histogram, Meter, UpDownCounter};
use opentelemetry::{InstrumentationScope, KeyValue, trace::TracerProvider};
use opentelemetry_otlp::{Compression, Protocol, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::metrics as sdkmetrics;
use opentelemetry_sdk::metrics::Temporality;
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace as sdktrace};
use opentelemetry_semantic_conventions as semvcns;
use std::sync::OnceLock;
use std::time::Duration;
use tonic::{metadata::MetadataMap, transport::ClientTlsConfig};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_telemetry()
-> anyhow::Result<(sdktrace::SdkTracerProvider, sdkmetrics::SdkMeterProvider)> {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    let tracer_provider = init_tracer()?;
    let meter_provider = init_metrics()?;
    let scope = InstrumentationScope::builder("ferroid")
        .with_version(env!("CARGO_PKG_VERSION"))
        .with_schema_url(semvcns::SCHEMA_URL)
        .build();

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "info,opentelemetry=warn".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_thread_ids(true)
                .with_line_number(true)
                .with_target(false)
                .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
                .with_file(true)
                .pretty(),
        )
        .with(
            tracing_opentelemetry::layer()
                .with_tracer(tracer_provider.tracer_with_scope(scope.clone()))
                .with_error_records_to_exceptions(true),
        )
        .with(tracing_opentelemetry::MetricsLayer::new(
            meter_provider.clone(),
        ))
        .init();

    opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    let meter = opentelemetry::global::meter_with_scope(scope);

    init_metric_handles(meter);

    Ok((tracer_provider, meter_provider))
}

fn get_metadata() -> anyhow::Result<MetadataMap> {
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

fn init_metrics() -> anyhow::Result<sdkmetrics::SdkMeterProvider> {
    let metadata: MetadataMap = get_metadata()?;

    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_tls_config(ClientTlsConfig::new().with_webpki_roots())
        .with_metadata(metadata)
        .with_timeout(Duration::from_secs(10))
        .with_compression(Compression::Gzip)
        .with_endpoint("https://api.eu1.honeycomb.io:443")
        .with_protocol(Protocol::Grpc)
        .with_temporality(Temporality::Delta)
        .build()
        .context("failed to build metrics exporter")?;

    Ok(sdkmetrics::SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(resource())
        .build())
}

fn init_tracer() -> anyhow::Result<sdktrace::SdkTracerProvider> {
    let metadata: MetadataMap = get_metadata()?;

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_tls_config(ClientTlsConfig::new().with_webpki_roots())
        .with_metadata(metadata)
        .with_timeout(Duration::from_secs(10))
        .with_compression(Compression::Gzip)
        .with_endpoint("https://api.eu1.honeycomb.io:443")
        .with_protocol(Protocol::Grpc)
        .build()
        .context("failed to build tracer exporter")?;

    let batch = sdktrace::BatchSpanProcessor::builder(exporter)
        .with_batch_config(
            sdktrace::BatchConfigBuilder::default()
                .with_scheduled_delay(Duration::from_secs(5))
                .with_max_queue_size(2048)
                .build(),
        )
        .build();

    Ok(sdktrace::SdkTracerProvider::builder()
        .with_span_processor(batch)
        .with_resource(resource())
        .build())
}

pub static REQUESTS: OnceLock<Counter<u64>> = OnceLock::new();
pub static STREAMS_INFLIGHT: OnceLock<UpDownCounter<i64>> = OnceLock::new();
pub static STREAM_ERRORS: OnceLock<Counter<u64>> = OnceLock::new();
pub static STREAM_DURATION_MS: OnceLock<Histogram<f64>> = OnceLock::new();
pub static IDS_GENERATED: OnceLock<Counter<u64>> = OnceLock::new();
pub static IDS_PER_REQUEST: OnceLock<Histogram<f64>> = OnceLock::new();

fn init_metric_handles(meter: Meter) {
    REQUESTS
        .set(
            meter
                .u64_counter("requests")
                .with_description("Total gRPC stream requests")
                .build(),
        )
        .ok();
    STREAMS_INFLIGHT
        .set(
            meter
                .i64_up_down_counter("streams_inflight")
                .with_description("Concurrent gRPC streams")
                .build(),
        )
        .ok();
    STREAM_ERRORS
        .set(
            meter
                .u64_counter("errors")
                .with_description("Errored/cancelled streams")
                .build(),
        )
        .ok();
    STREAM_DURATION_MS
        .set(
            meter
                .f64_histogram("stream_duration")
                .with_unit("ms")
                .with_description("End-to-end stream duration")
                .build(),
        )
        .ok();
    IDS_GENERATED
        .set(
            meter
                .u64_counter("ids_generated")
                .with_description("Total Snowflake IDs generated")
                .build(),
        )
        .ok();
    IDS_PER_REQUEST
        .set(
            meter
                .f64_histogram("ids_per_request")
                .with_description("IDs requested per stream")
                .build(),
        )
        .ok();
}
