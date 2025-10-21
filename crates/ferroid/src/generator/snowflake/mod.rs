#[cfg(all(feature = "atomic", target_has_atomic = "64"))]
mod atomic;
#[cfg(feature = "basic")]
mod basic;
mod interface;
#[cfg(feature = "lock")]
mod lock;
#[cfg(all(
    test,
    feature = "std",
    feature = "alloc",
    feature = "basic",
    feature = "lock",
    feature = "atomic"
))]
mod tests;

#[cfg_attr(
    docsrs,
    doc(cfg(all(feature = "snowflake", feature = "atomic", target_has_atomic = "64")))
)]
#[cfg(all(feature = "atomic", target_has_atomic = "64"))]
pub use atomic::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "snowflake", feature = "basic"))))]
#[cfg(feature = "basic")]
pub use basic::*;
#[cfg_attr(docsrs, doc(cfg(feature = "snowflake")))]
pub use interface::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "snowflake", feature = "lock"))))]
#[cfg(feature = "lock")]
pub use lock::*;
