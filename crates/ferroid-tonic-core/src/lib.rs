#![doc = include_str!("../README.md")]

mod common;
pub use common::*;
// Public re-export so downstream crates can access `ferroid` via
// `ferroid_tonic_core::ferroid`
pub use ferroid;
