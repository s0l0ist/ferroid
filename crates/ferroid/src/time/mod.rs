// CachedClock requires a background thread and shared 64-bit atomic storage.
// wasm32-unknown-unknown does not support standard threads.
#[cfg(all(
    feature = "std",
    not(all(target_arch = "wasm32", target_os = "unknown")),
    target_has_atomic = "ptr",
    target_has_atomic = "64"
))]
mod cached_clock;
#[cfg(feature = "std")]
mod direct_clock;
mod interface;
#[cfg(feature = "std")]
pub(crate) mod system_time;

#[cfg(all(
    feature = "std",
    not(all(target_arch = "wasm32", target_os = "unknown")),
    target_has_atomic = "ptr",
    target_has_atomic = "64"
))]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub use cached_clock::*;
#[cfg(feature = "std")]
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
pub use direct_clock::*;
pub use interface::*;
