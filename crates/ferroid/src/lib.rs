#![doc = include_str!("../README.md")]
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(not(any(feature = "snowflake", feature = "ulid")))]
compile_error!("Enable at least one ID family: `snowflake` or `ulid`.");

#[cfg(any(feature = "std", test))]
extern crate std;

#[cfg(any(feature = "alloc", test))]
extern crate alloc;

// Implementation details used by exported macros.
#[cfg(any(feature = "std", feature = "alloc"))]
#[doc(hidden)]
pub mod __internal {
    #[cfg(feature = "alloc")]
    pub use alloc::string::String;

    #[cfg(feature = "std")]
    pub use std::time::SystemTime;

    #[cfg(feature = "std")]
    pub use crate::time::system_time::system_time_now;
}

#[cfg(feature = "base32")]
pub mod base32;
mod error;
#[cfg(feature = "futures")]
pub mod futures;
pub mod generator;
pub mod id;
pub mod rand;
#[cfg(feature = "serde")]
pub mod serde;
pub mod time;

pub use error::Error;
