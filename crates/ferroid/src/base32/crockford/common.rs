//! Shared alphabet and error reporting.

use crate::{base32::Error, generator::Result};

pub(super) const INVALID_VALUE: u8 = 255;
pub(super) const BITS_PER_CHAR: usize = 5;

/// Lookup table for Crockford Base32 encoding.
pub(super) static ENCODE_LUT: [u8; 32] = *b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Lookup table for Crockford Base32 decoding.
pub(super) static DECODE_LUT: [u8; 256] = {
    let mut lut = [INVALID_VALUE; 256];
    let mut i = 0_u8;
    // Accept both cases of the main alphabet.
    while i < 32 {
        let c = ENCODE_LUT[i as usize];
        lut[c as usize] = i;
        if c.is_ascii_uppercase() {
            lut[(c + 32) as usize] = i; // lowercase letter
        }
        i += 1;
    }
    // Accept Crockford aliases for zero and one.
    lut[b'O' as usize] = 0;
    lut[b'o' as usize] = 0;
    lut[b'I' as usize] = 1;
    lut[b'i' as usize] = 1;
    lut[b'L' as usize] = 1;
    lut[b'l' as usize] = 1;
    lut
};

// Uppercase ASCII '0'..='_' fits in three vectors. Derive this table from the
// byte lookup table so alphabet changes and aliases have one source of truth.
// Valid entries store value + 1; zero denotes invalid input. SIMD kernels also
// zero out-of-range lookups. Subtracting one restores 0..31 or invalid value 255.
#[cfg(all(
    feature = "intrinsics",
    any(
        all(target_arch = "aarch64", target_feature = "neon"),
        all(target_arch = "x86_64", target_feature = "ssse3"),
    )
))]
pub(super) const DECODE_COMPACT: [u8; 48] = {
    let mut table = [0; 48];
    let mut index = 0;
    while index < table.len() {
        let value = DECODE_LUT[b'0' as usize + index];
        if value != INVALID_VALUE {
            table[index] = value + 1;
        }
        index += 1;
    }
    table
};

/// Validates the original bytes on the cold path, reporting the first error.
#[cold]
#[inline(never)]
pub(super) fn validate_symbols<ID>(bytes: &[u8]) -> Result<(), Error<ID>> {
    for (i, &b) in bytes.iter().enumerate() {
        // SAFETY:
        // - b is a u8, so its index is in 0..=255.
        // - DECODE_LUT has 256 initialized entries and is only read.
        let v = unsafe { *DECODE_LUT.get_unchecked(b as usize) };
        if v == INVALID_VALUE {
            return Err(Error::DecodeInvalidAscii { byte: b, index: i });
        }
    }
    Ok(())
}
