#![doc = include_str!("../README.md")]
#![no_std]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "base32")]
mod base32;
mod error;
#[cfg(feature = "futures")]
mod futures;
#[cfg(any(feature = "snowflake", feature = "ulid"))]
mod generator;
mod id;
#[cfg(all(feature = "std", feature = "alloc"))]
mod mono_clock;
#[cfg(feature = "std")]
mod mutex;
mod rand;
#[cfg(any(feature = "async-tokio", feature = "async-smol"))]
mod runtime;
mod status;
#[cfg(feature = "std")]
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
#[cfg(all(feature = "std", feature = "alloc"))]
pub use crate::mono_clock::*;
pub use crate::rand::*;
#[cfg(any(feature = "async-tokio", feature = "async-smol"))]
pub use crate::runtime::*;
pub use crate::status::*;
#[cfg(feature = "std")]
pub use crate::thread_random::*;
pub use crate::time::*;
#[cfg(feature = "std")]
pub use mutex::*;
