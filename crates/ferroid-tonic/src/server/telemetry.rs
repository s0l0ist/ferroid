/// Initializes structured logging using `tracing-subscriber`.
///
/// This is a no-op if the `tracing` feature is disabled.
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
