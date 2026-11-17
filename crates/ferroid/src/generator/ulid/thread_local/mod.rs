//! Thread-local ULID generation utilities.
//!
//! Provides non-monotonic and monotonic ULID generation using thread-local
//! generators with a choice of clock:
//!
//! - [`direct::Ulid`] is the usual choice. It reads wall time on every call
//!   and starts no background thread.
//! - [`cached::Ulid`] uses [`CachedClock`], which refreshes a shared timestamp
//!   roughly once per millisecond on a background thread. It requires thread
//!   support and is useful when profiling shows clock reads are a bottleneck.
//!
//! Both expose the same API. Import the chosen module and use `direct::Ulid`
//! or `cached::Ulid` explicitly. Prefer `direct::Ulid` unless profiling shows
//! that cached clock reads benefit your workload. Use `direct::Ulid` on
//! targets without thread support, including `wasm32-unknown-unknown`.
//!
//! Monotonic generators invoke the backoff callback when the random component
//! is exhausted or wall time moves behind the last generated timestamp. The
//! callback receives a suggested [`core::time::Duration`], including zero.
//! After a backwards clock correction, generation waits until wall time catches up.
//!
//! [`CachedClock`]: crate::time::CachedClock
//! [`DirectClock`]: crate::time::DirectClock

#[cfg(all(
    not(all(target_arch = "wasm32", target_os = "unknown")),
    target_has_atomic = "ptr",
    target_has_atomic = "64"
))]
pub mod cached;
pub mod direct;
