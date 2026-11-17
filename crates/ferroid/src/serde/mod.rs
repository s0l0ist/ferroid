//! Serde adapters for any type implementing [`crate::id::Id`].
//!
//! Use `as_int` for the native integer or `as_base32` for a Crockford Base32
//! string. Both validate the ID on deserialization; serialization preserves the
//! supplied value without normalization. Deserialization requires a checked
//! `TryFrom` implementation returning [`crate::Error`].

#[cfg(feature = "base32")]
pub mod as_base32;
pub mod as_int;

pub use crate::Error;

#[cfg(test)]
mod tests;
