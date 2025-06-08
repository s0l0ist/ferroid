use opentelemetry::{InstrumentationScope, KeyValue, trace::TracerProvider};
use opentelemetry_otlp::{Compression, Protocol, WithExportConfig, WithTonicConfig};
use opentelemetry_sdk::metrics as sdkmetrics;
use opentelemetry_sdk::metrics::{Stream, Temporality};
use opentelemetry_sdk::{Resource, propagation::TraceContextPropagator, trace as sdktrace};
use opentelemetry_semantic_conventions as semvcns;
use std::time::Duration;
use tonic::{metadata::MetadataMap, transport::ClientTlsConfig};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn init_telemetry() -> (sdktrace::SdkTracerProvider, sdkmetrics::SdkMeterProvider) {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    let tracer_provider = init_tracer();
    let meter_provider = init_metrics();

    // Configure the registry and auto set it as the global subscriber
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
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
                .with_tracer(
                    tracer_provider.tracer_with_scope(
                        InstrumentationScope::builder("ferroid")
                            .with_version(env!("CARGO_PKG_VERSION"))
                            .with_schema_url(semvcns::SCHEMA_URL)
                            .build(),
                    ),
                )
                .with_error_records_to_exceptions(true),
        )
        .with(tracing_opentelemetry::MetricsLayer::new(
            meter_provider.clone(),
        ))
        .init();

    opentelemetry::global::set_tracer_provider(tracer_provider.clone());
    opentelemetry::global::set_meter_provider(meter_provider.clone());

    (tracer_provider, meter_provider)
}

fn get_metadata() -> MetadataMap {
    let mut map = MetadataMap::new();
    map.insert(
        "x-honeycomb-team",
        std::env::var("HONEYCOMB_API_KEY")
            .expect("missing `HONEYCOMB_API_KEY`")
            .parse()
            .expect("Failed to parse as metadata"),
    );
    map
}
fn init_tracer() -> sdktrace::SdkTracerProvider {
    let metadata: MetadataMap = get_metadata();

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_tls_config(ClientTlsConfig::new().with_webpki_roots())
        .with_metadata(metadata)
        .with_timeout(Duration::from_secs(10))
        .with_compression(Compression::Gzip)
        .with_endpoint("https://api.eu1.honeycomb.io:443")
        .with_protocol(Protocol::Grpc)
        .build()
        .expect("failed to build exporter");

    let batch = sdktrace::BatchSpanProcessor::builder(exporter)
        .with_batch_config(
            sdktrace::BatchConfigBuilder::default()
                .with_scheduled_delay(Duration::from_secs(5))
                .with_max_queue_size(2048)
                .build(),
        )
        .build();

    sdktrace::SdkTracerProvider::builder()
        .with_span_processor(batch)
        .with_resource(resource())
        .build()
}

fn init_metrics() -> sdkmetrics::SdkMeterProvider {
    let metadata: MetadataMap = get_metadata();

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
        .expect("failed to build OTLP metrics exporter");

    let latency_view = |i: &sdkmetrics::Instrument| {
        if i.name() == "ferroid.stream_duration_ms" {
            Some(Stream::builder().with_unit("ms").build().unwrap())
        } else {
            None
        }
    };

    let error_view = |i: &sdkmetrics::Instrument| {
        if i.name() == "ferroid.chunk_send_errors" {
            Some(Stream::builder().build().unwrap())
        } else {
            None
        }
    };
    sdkmetrics::SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .with_resource(resource())
        .with_view(latency_view)
        .with_view(error_view)
        .build()
}

// Create a Resource that captures information about the entity for which telemetry is recorded.
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
