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
#[cfg(feature = "std")]
mod mutex;
pub mod rand;

pub mod time;

#[cfg_attr(docsrs, doc(cfg(feature = "base32")))]
#[cfg(feature = "base32")]
pub mod base32;
pub mod id;
pub use crate::error::*;
#[cfg_attr(docsrs, doc(cfg(any(feature = "snowflake", feature = "ulid"))))]
#[cfg(any(feature = "snowflake", feature = "ulid"))]
pub mod generator;
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
#[cfg(feature = "serde")]
pub mod serde;
#[cfg_attr(docsrs, doc(cfg(feature = "std")))]
#[cfg(feature = "std")]
pub use mutex::*;
