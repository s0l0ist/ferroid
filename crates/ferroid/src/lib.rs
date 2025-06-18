#![doc = include_str!("../README.md")]

#[cfg(not(any(feature = "snowflake", feature = "ulid")))]
compile_error!("You must enable at least one of the following features: 'snowflake' or 'ulid'.");

#[cfg(feature = "base32")]
mod base32;
mod error;
#[cfg(feature = "futures")]
mod futures;
#[cfg(any(feature = "snowflake", feature = "ulid"))]
mod generator;
mod id;
mod mono_clock_native;
#[cfg(feature = "ulid")]
mod rand;
mod runtime;
mod status;
#[cfg(feature = "ulid")]
mod thread_random;
mod time;

#[cfg(feature = "base32")]
pub use crate::base32::*;
pub use crate::error::*;
#[cfg(feature = "futures")]
pub use crate::futures::*;
#[cfg(any(feature = "snowflake", feature = "ulid"))]
pub use crate::generator::*;
pub use crate::id::*;
pub use crate::mono_clock_native::*;
#[cfg(feature = "ulid")]
pub use crate::rand::*;
#[cfg(any(feature = "async-tokio", feature = "async-smol"))]
pub use crate::runtime::*;
pub use crate::status::*;
#[cfg(feature = "ulid")]
pub use crate::thread_random::*;
pub use crate::time::*;
