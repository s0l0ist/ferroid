//! Crockford Base32 encoding and decoding.
//!
//! The `intrinsics` feature allows build-target CPU features to select native
//! kernels for 64- and 128-bit IDs. Disabling it selects scalar on every target.
//! 32-bit IDs and other targets use scalar, in both `std` and `no_std`.

mod common;
mod interface;
#[cfg(all(
    feature = "intrinsics",
    target_arch = "aarch64",
    target_feature = "neon"
))]
mod neon;
mod scalar;
#[cfg(all(
    feature = "intrinsics",
    target_arch = "x86_64",
    target_feature = "ssse3"
))]
mod ssse3;

pub(super) use interface::{decode_base32, encode_base32};
