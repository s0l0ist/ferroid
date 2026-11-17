//! Crockford Base32 encoding and decoding for ferroid IDs.
//!
//! [`Base32Ext`] provides encoding and validated decoding
//! for built-in and custom IDs, backed by shared formatters. The default
//! `intrinsics` feature and build-target CPU features select native kernels,
//! with scalar fallbacks for 32-bit IDs and other targets. Disabling `intrinsics`
//! selects scalar on every target.
//!
//! # Base32 Overflow Details
//!
//! Base32 encodes in 5-bit chunks. That means:
//!
//! - A `u32` (32 bits) maps to 7 Base32 characters (7 × 5 = 35 bits)
//! - A `u64` (64 bits) maps to 13 Base32 characters (13 × 5 = 65 bits)
//! - A `u128` (128 bits) maps to 26 Base32 characters (26 × 5 = 130 bits)
//!
//! This creates an invariant: an encoded string may contain more bits than the
//! target type can hold.
//!
//! ## ULID Specification vs. Ferroid
//!
//! The [ULID
//! specification](https://github.com/ulid/spec?tab=readme-ov-file#overflow-errors-when-parsing-base32-strings)
//! is strict:
//!
//! > Technically, a 26-character Base32 encoded string can contain 130 bits of
//! > information, whereas a ULID must only contain 128 bits. Therefore, the largest
//! > valid ULID encoded in Base32 is 7ZZZZZZZZZZZZZZZZZZZZZZZZZ, which corresponds
//! > to an epoch time of 281474976710655 or 2 ^ 48 - 1.
//! >
//! > Any attempt to decode or encode a ULID larger than this should be rejected by
//! > all implementations, to prevent overflow bugs.
//!
//! Ferroid decodes into the backing integer first, discarding high bits beyond
//! its width. It then checks the remaining integer's reserved layout bits:
//!
//! - Strings like `"ZZZZZZZZZZZZZZZZZZZZZZZZZZ"` (which technically overflow) are
//!   accepted for `ULID`, whose layout has no reserved bits
//! - If any reserved bits in the remaining integer are set, decoding fails with
//!   `ferroid::base32::Error::DecodeOverflow`
//!
//! This allows any 13-character Base32 string to decode into a `u64`, or any
//! 26-character string into a `u128`, **as long as reserved layout constraints
//! aren't violated**. If the layout defines no reserved bits, decoding is always
//! considered valid, provided the input has the correct length and valid
//! Crockford Base32 characters. Distinct strings can decode to the same ID;
//! encoding the result produces its canonical representation.
//!
//! For example:
//!
//! - A `ULID` has no reserved bits, so decoding will never fail due to overflow
//! - A `SnowflakeTwitterId` reserves the highest bit, so decoding must ensure that
//!   bit remains unset
//!
//! If reserved bits are set during decoding, Ferroid returns a
//! `ferroid::base32::Error::DecodeOverflow { id }` containing the full (invalid)
//! ID after truncation. You can recover by calling `.into_valid()` to mask off reserved
//! bits—allowing either explicit error handling or silent correction.

mod crockford;
mod formatter;
mod interface;
#[cfg(test)]
mod tests;

pub use crate::Error;
use crockford::{decode_base32, encode_base32};
pub use formatter::*;
pub use interface::Base32Ext;
