#[cfg(feature = "async-smol")]
mod smol;
#[cfg(feature = "async-tokio")]
mod tokio;

#[cfg(all(feature = "async-smol", feature = "snowflake"))]
mod smol_snowflake;
#[cfg(all(feature = "async-smol", feature = "ulid"))]
mod smol_ulid;
#[cfg(all(feature = "async-tokio", feature = "snowflake"))]
mod tokio_snowflake;
#[cfg(all(feature = "async-tokio", feature = "ulid"))]
mod tokio_ulid;

#[cfg_attr(docsrs, doc(cfg(feature = "async-smol")))]
#[cfg(feature = "async-smol")]
pub use smol::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "async-smol", feature = "snowflake"))))]
#[cfg(all(feature = "async-smol", feature = "snowflake"))]
pub use smol_snowflake::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "async-smol", feature = "ulid"))))]
#[cfg(all(feature = "async-smol", feature = "ulid"))]
pub use smol_ulid::*;
#[cfg_attr(docsrs, doc(cfg(feature = "async-tokio")))]
#[cfg(feature = "async-tokio")]
pub use tokio::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "async-tokio", feature = "snowflake"))))]
#[cfg(all(feature = "async-tokio", feature = "snowflake"))]
pub use tokio_snowflake::*;
#[cfg_attr(docsrs, doc(cfg(all(feature = "async-tokio", feature = "ulid"))))]
#[cfg(all(feature = "async-tokio", feature = "ulid"))]
pub use tokio_ulid::*;
