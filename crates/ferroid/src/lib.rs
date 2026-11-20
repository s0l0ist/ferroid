//! ULID and Snowflake-style identifiers with configurable layouts, synchronous
//! and asynchronous generation, Base32 encoding, and Serde integration.
//!
//! Use [`id`] for ID types and layout traits, [`generator`] for generation
//! backends, and [`time`] and [`rand`] for clocks and random sources.
//!
//! # Quick start
//!
//! Enable `ulid` and `thread-rng` to generate a standard ULID with fresh
//! randomness and the current wall time:
//!
//! ```
//! # #[cfg(all(feature = "ulid", feature = "thread-rng"))]
//! # {
//! use ferroid::id::ULID;
//!
//! let id = ULID::now();
//! # }
//! ```
//!
//! # Features
//!
//! Only `intrinsics` is enabled by default. Select the capabilities you need:
//!
//! - `ulid` and `snowflake`: ID families and custom layout macros.
//! - `basic`, `atomic`, and `lock`: generator backends.
//! - `std`: built-in clocks and system-time conversions; usable without an ID family.
//! - `thread-rng` and `thread-local`: built-in randomness and thread-local ULID generators.
//! - `base32`, `serde`, and `futures`: encoding, serialization, and async generation.
//! - `alloc`: owned string helpers. Core APIs support `no_std` with custom providers.
//!
//! See the [README](https://github.com/s0l0ist/ferroid/blob/main/crates/ferroid/README.md)
//! for setup, examples, feature details, and benchmarks, and the
//! [changelog](https://github.com/s0l0ist/ferroid/blob/main/CHANGELOG.md) for release notes.
#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[cfg(any(feature = "std", test))]
extern crate std;

#[cfg(any(feature = "alloc", test))]
extern crate alloc;

#[macro_use]
mod cfg_select;

// Check README examples without including the README on the crate's API page.
#[cfg(doctest)]
mod readme_tests;

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
