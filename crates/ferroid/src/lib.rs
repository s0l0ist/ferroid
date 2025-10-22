#![doc = include_str!("../README.md")]
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "base32")]
pub mod base32;
#[cfg(feature = "futures")]
pub mod futures;
pub mod generator;
pub mod id;
pub mod rand;
#[cfg(feature = "serde")]
pub mod serde;
pub mod time;
