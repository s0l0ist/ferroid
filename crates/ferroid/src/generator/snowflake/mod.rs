#[cfg(all(
    feature = "atomic",
    any(target_has_atomic = "64", target_has_atomic = "128")
))]
mod atomic;
#[cfg(feature = "basic")]
mod basic;
mod interface;
#[cfg(feature = "lock")]
mod lock;

#[cfg_attr(
    docsrs,
    doc(cfg(all(
        feature = "snowflake",
        feature = "atomic",
        any(target_has_atomic = "64", target_has_atomic = "128")
    )))
)]
#[cfg(all(
    feature = "atomic",
    any(target_has_atomic = "64", target_has_atomic = "128")
))]
pub use atomic::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "snowflake", feature = "basic"))))]
#[cfg(feature = "basic")]
pub use basic::*;
#[cfg_attr(docsrs, doc(cfg(feature = "snowflake")))]
pub use interface::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "snowflake", feature = "lock"))))]
#[cfg(feature = "lock")]
pub use lock::*;

#[cfg(all(
    test,
    any(
        feature = "basic",
        feature = "lock",
        all(
            feature = "atomic",
            any(target_has_atomic = "64", target_has_atomic = "128")
        )
    )
))]
mod tests;
