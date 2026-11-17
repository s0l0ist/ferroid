//! Executor-independent async generation (`futures` feature).
//!
//! Supply an async backoff strategy, invoked only when generation is pending.
//! The generator passes a suggested [`core::time::Duration`], with clock
//! granularity already applied, and awaits the callback before retrying.
//!
//! For timed async backoff, pass a sleep function such as `tokio::time::sleep`.

#[cfg(feature = "snowflake")]
mod snowflake;
#[cfg(feature = "ulid")]
mod ulid;

#[cfg(feature = "snowflake")]
#[cfg_attr(docsrs, doc(cfg(all(feature = "futures", feature = "snowflake"))))]
pub use snowflake::*;
#[cfg(feature = "ulid")]
#[cfg_attr(docsrs, doc(cfg(all(feature = "futures", feature = "ulid"))))]
pub use ulid::*;

#[cfg(all(test, feature = "basic", any(feature = "snowflake", feature = "ulid")))]
mod tests;
