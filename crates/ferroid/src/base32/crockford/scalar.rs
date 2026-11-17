//! Portable Crockford Base32 encoding and decoding.

use super::common::{BITS_PER_CHAR, DECODE_LUT, ENCODE_LUT, INVALID_VALUE};
use crate::id::BeBytes;

/// Encodes big-endian bytes as fixed-width Crockford Base32 using scalar code.
///
/// Sealed [`BeBytes`] implementations provide matching fixed-size arrays.
#[inline(always)]
#[allow(clippy::inline_always)]
pub(super) fn encode_base32<T: BeBytes>(input: &T::ByteArray, output: &mut T::Base32Array) {
    let input = input.as_ref();
    let buf_slice = output.as_mut();
    let input_bits = T::SIZE * 8;
    let total_bits = T::BASE32_SIZE * BITS_PER_CHAR;
    let mut bits = total_bits - input_bits;
    let mut acc = 0;
    let mask = 0x1F;

    let mut out = 0;
    for &b in input {
        acc = (acc << 8) | usize::from(b);
        bits += 8;
        while bits >= BITS_PER_CHAR {
            bits -= BITS_PER_CHAR;
            // SAFETY:
            // - `(acc >> bits) & mask` is in 0..=31, within the 32-entry LUT.
            // - The immutable lookup returns a copied byte, retaining no borrow.
            buf_slice[out] = unsafe { *ENCODE_LUT.get_unchecked((acc >> bits) & mask) };
            out += 1;
        }
    }
}

/// Decodes symbols and accumulates an error flag without per-character branches.
///
/// The shared interface discards the decoded value when an error is flagged.
#[inline(always)]
#[allow(clippy::inline_always)]
pub(super) fn decode_base32<T>(bytes: &[u8]) -> (T, bool)
where
    T: Default + From<u8> + core::ops::Shl<usize, Output = T> + core::ops::BitOr<Output = T>,
{
    let mut acc = T::default();
    let mut has_error = false;

    let (chunks, remainder) = bytes.as_chunks::<4>();
    for &[b0, b1, b2, b3] in chunks {
        // SAFETY:
        // - Each input is a u8, so every index is in 0..=255.
        // - DECODE_LUT has 256 initialized entries and is only read.
        let (v0, v1, v2, v3) = unsafe {
            let v0 = *DECODE_LUT.get_unchecked(b0 as usize);
            let v1 = *DECODE_LUT.get_unchecked(b1 as usize);
            let v2 = *DECODE_LUT.get_unchecked(b2 as usize);
            let v3 = *DECODE_LUT.get_unchecked(b3 as usize);

            (v0, v1, v2, v3)
        };
        acc = (acc << BITS_PER_CHAR) | T::from(v0);
        acc = (acc << BITS_PER_CHAR) | T::from(v1);
        acc = (acc << BITS_PER_CHAR) | T::from(v2);
        acc = (acc << BITS_PER_CHAR) | T::from(v3);

        has_error |= (v0 | v1 | v2 | v3) == INVALID_VALUE;
    }

    for &b in remainder {
        // SAFETY:
        // - b is a u8, so its index is in 0..=255.
        // - DECODE_LUT has 256 initialized entries and is only read.
        let v = unsafe { *DECODE_LUT.get_unchecked(b as usize) };
        acc = (acc << BITS_PER_CHAR) | T::from(v);
        has_error |= v == INVALID_VALUE;
    }

    (acc, has_error)
}
