//! Telemetry and diagnostics initialization for the ID generation service.
//!
//! This module sets up structured logging using the `tracing` ecosystem. When
//! compiled with the `debug` feature, it enables human-readable, file-aware,
//! thread-tagged logs that respect environment-based filtering.
//!
//! ## Behavior
//!
//! - Uses `tracing-subscriber` with pretty-printed output.
//! - Pulls filtering rules from `RUST_LOG` or defaults to `info`.
//! - Includes thread ID, file, and line number for traceability.
//! - Timestamped using local time (RFC 3339 format).
//!
//! ## When Disabled
//!
//! If compiled **without** the `debug` feature, this function becomes a no-op.
//! This allows the server to run silently in production or CI unless explicitly
//! instrumented with tracing.

/// Initializes structured logging via `tracing-subscriber`, if enabled.
///
/// This function configures the default global subscriber with:
/// - Environment-based log level filtering (via `RUST_LOG`)
/// - Pretty-printed span and event formatting
/// - File/line/thread metadata for diagnostics
///
/// No effect unless the `debug` feature is enabled at compile time.
pub fn init_tracing() {
    #[cfg(feature = "debug")]
    {
        use tracing_subscriber::fmt::format::FmtSpan;
        use tracing_subscriber::{EnvFilter, fmt};

        fmt()
            .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
            .with_span_events(FmtSpan::NONE)
            .with_target(false)
            .with_thread_ids(true)
            .with_file(true)
            .with_line_number(true)
            .with_timer(fmt::time::ChronoLocal::rfc_3339())
            .pretty()
            .init();
    }
}
