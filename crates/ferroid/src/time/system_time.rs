/// Returns browser wall time on `wasm32-unknown-unknown`, or the standard
/// system clock on other targets.
#[must_use]
pub fn system_time_now() -> std::time::SystemTime {
    core::cfg_select! {
        all(target_arch = "wasm32", target_os = "unknown") => {
            use web_time::web::SystemTimeExt;
            web_time::SystemTime::now().to_std()
        }
        _ => std::time::SystemTime::now(),
    }
}
