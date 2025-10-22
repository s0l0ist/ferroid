#![doc = include_str!("../README.md")]
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

mod error;
#[cfg(feature = "futures")]
pub mod futures;
#[cfg(any(feature = "snowflake", feature = "ulid"))]
mod generator;
#[cfg(all(feature = "std", feature = "alloc", target_has_atomic = "64"))]
mod mono_clock;
#[cfg(feature = "std")]
mod mutex;
mod rand;
mod status;
#[cfg(feature = "std")]
mod thread_random;
mod time;

#[cfg_attr(docsrs, doc(cfg(feature = "base32")))]
#[cfg(feature = "base32")]
pub mod base32;
pub mod id;
pub use crate::error::*;
#[cfg_attr(docsrs, doc(cfg(any(feature = "snowflake", feature = "ulid"))))]
#[cfg(any(feature = "snowflake", feature = "ulid"))]
pub use crate::generator::*;
#[cfg_attr(
    docsrs,
    doc(cfg(all(feature = "std", feature = "alloc", target_has_atomic = "64")))
)]
#[cfg(all(feature = "std", feature = "alloc", target_has_atomic = "64"))]
pub use crate::mono_clock::*;
pub use crate::rand::*;
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
#[cfg(feature = "serde")]
pub mod serde;
pub use crate::status::*;
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[cfg(feature = "std")]
pub use crate::thread_random::*;
pub use crate::time::*;
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[cfg(feature = "std")]
pub use mutex::*;
