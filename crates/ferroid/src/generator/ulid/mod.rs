#[cfg(all(feature = "atomic", target_has_atomic = "128"))]
mod atomic_mono;
#[cfg(feature = "basic")]
mod basic;
#[cfg(feature = "basic")]
mod basic_mono;
mod interface;
#[cfg(feature = "lock")]
mod lock_mono;
#[cfg(all(
    test,
    feature = "std",
    feature = "alloc",
    feature = "basic",
    feature = "lock",
    feature = "atomic"
))]
mod tests;
#[cfg(feature = "thread-local")]
mod thread_local;

#[cfg_attr(
    docsrs,
    doc(cfg(all(feature = "ulid", feature = "atomic", target_has_atomic = "128")))
)]
#[cfg(all(feature = "atomic", target_has_atomic = "128"))]
pub use atomic_mono::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "ulid", feature = "basic"))))]
#[cfg(feature = "basic")]
pub use basic::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "ulid", feature = "basic"))))]
#[cfg(feature = "basic")]
pub use basic_mono::*;
#[cfg_attr(docsrs, doc(cfg(feature = "ulid")))]
pub use interface::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "ulid", feature = "lock"))))]
#[cfg(feature = "lock")]
pub use lock_mono::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "ulid", feature = "thread-local"))))]
#[cfg(feature = "thread-local")]
pub use thread_local::*;
