use crate::{base32::Error, generator::Result, id::BeBytes};

const INVALID_VALUE: u8 = 255;
const BITS_PER_CHAR: usize = 5;

/// Lookup table for Crockford base32 encoding
static ENCODE_LUT: [u8; 32] = *b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Lookup table for Crockford base32 decoding
static DECODE_LUT: [u8; 256] = {
    let mut lut = [INVALID_VALUE; 256];
    let mut i = 0_u8;
    // Main alphabet, allow lower-case
    while i < 32 {
        let c = ENCODE_LUT[i as usize];
        lut[c as usize] = i;
        if c.is_ascii_uppercase() {
            lut[(c + 32) as usize] = i; // lowercase letter
        }
        i += 1;
    }
    // Crockford-specific aliases
    lut[b'O' as usize] = 0;
    lut[b'o' as usize] = 0;
    lut[b'I' as usize] = 1;
    lut[b'i' as usize] = 1;
    lut[b'L' as usize] = 1;
    lut[b'l' as usize] = 1;
    lut
};

/// Encodes a byte slice into Crockford base32, writing output to `buf_slice`.
///
/// # Safety
///
/// - The caller must ensure that `buf_slice` is **exactly** the correct size to
///   hold the base32-encoded output. This is guaranteed at compile time when
///   encoding fixed-size inputs which we ensure in the caller when using
///   `Base32Array`.
///
/// - The index into `ENCODE_LUT` is masked with `0x1F` (31), ensuring it is
///   always in the range 0..=31.
///   - `ENCODE_LUT` is a fixed-size array with exactly 32 elements, so all
///     masked indices are valid.
///   - Therefore, `ENCODE_LUT[(acc >> bits) & 0x1F]` is guaranteed to be
///     in-bounds.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn encode_base32(input: &[u8], buf_slice: &mut [u8]) {
    let input_bits = input.len() * 8;
    let output_chars = buf_slice.len();
    let total_bits = output_chars * BITS_PER_CHAR;
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
            // - `out` is strictly less than `buf_slice.len()`, by caller's
            //    contract.
            // - `(acc >> bits) & mask` produces values in the range 0..=31.
            unsafe {
                *buf_slice.get_unchecked_mut(out) = *ENCODE_LUT.get_unchecked((acc >> bits) & mask);
            }
            out += 1;
        }
    }
}

/// Decodes a fixed-length Crockford base32 string into the given integer type
/// `T`.
///
/// Returns an error if the input contains invalid characters.
///
/// # Safety
///
/// - `encoded.bytes()` produces values in the range 0..=255.
/// - `DECODE_LUT` is a fixed-size array of 256 elements, so `DECODE_LUT[b as
///   usize]` is always in-bounds.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn decode_base32<T, E>(bytes: &[u8]) -> Result<T, Error<E>>
where
    T: BeBytes
        + Default
        + From<u8>
        + core::ops::Shl<usize, Output = T>
        + core::ops::BitOr<Output = T>,
{
    let mut acc = T::default();
    let mut has_error = false;

    let chunks = bytes.chunks_exact(4);
    let remainder = chunks.remainder();
    for chunk in chunks {
        let (v0, v1, v2, v3) = unsafe {
            // SAFETY: `chunks_exact(X)` guarantees `chunk.len() == X`
            let b0 = *chunk.get_unchecked(0);
            let b1 = *chunk.get_unchecked(1);
            let b2 = *chunk.get_unchecked(2);
            let b3 = *chunk.get_unchecked(3);

            // SAFETY: `bX as usize` is in 0..=255, and `DECODE_LUT` has 256
            // entries.
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

    // Handle remainder
    for &b in remainder {
        // SAFETY: `b as usize` is in 0..=255, and `DECODE_LUT` has 256 entries.
        let v = unsafe { *DECODE_LUT.get_unchecked(b as usize) };
        acc = (acc << BITS_PER_CHAR) | T::from(v);
        has_error |= v == INVALID_VALUE;
    }

    if has_error {
        return Err(handle_error(bytes));
    }
    Ok(acc)
}

#[cold]
#[inline(never)]
fn handle_error<E>(bytes: &[u8]) -> Error<E> {
    for (i, &b) in bytes.iter().enumerate() {
        // SAFETY: `b as usize` is in 0..=255, and `DECODE_LUT` has 256 entries.
        let v = unsafe { *DECODE_LUT.get_unchecked(b as usize) };
        if v == INVALID_VALUE {
            return Error::DecodeInvalidAscii { byte: b, index: i };
        }
    }
    unreachable!("handle_error called but no invalid byte found");
}
#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_u32(val: u32) {
        let bytes = val.to_be_bytes();
        let mut buf = [0u8; 7]; // ceil(32/5) = 7 chars for u32
        encode_base32(&bytes, &mut buf);
        let decoded = decode_base32::<u32, ()>(&buf).unwrap();
        let s = core::str::from_utf8(&buf).unwrap();
        assert_eq!(val, decoded, "roundtrip for u32: input={val}, b32={s}");
    }

    fn roundtrip_u64(val: u64) {
        let bytes = val.to_be_bytes();
        let mut buf = [0u8; 13]; // ceil(64/5) = 13 chars for u64
        encode_base32(&bytes, &mut buf);
        let decoded = decode_base32::<u64, ()>(&buf).unwrap();
        let s = core::str::from_utf8(&buf).unwrap();
        assert_eq!(val, decoded, "roundtrip for u64: input={val}, b32={s}");
    }

    fn roundtrip_u128(val: u128) {
        let bytes = val.to_be_bytes();
        let mut buf = [0u8; 26]; // ceil(128/5) = 26 chars for u128
        encode_base32(&bytes, &mut buf);
        let decoded = decode_base32::<u128, ()>(&buf).unwrap();
        let s = core::str::from_utf8(&buf).unwrap();
        assert_eq!(val, decoded, "roundtrip for u128: input={val}, b32={s}");
    }

    #[test]
    fn encode_decode_preserves_u32_values() {
        for &v in &[0, 1, u32::MAX, u32::MIN, 42, 0xFF00_FF00, 0x1234_5678] {
            roundtrip_u32(v);
        }
    }

    #[test]
    fn encode_decode_preserves_u64_values() {
        for &v in &[
            0,
            1,
            u64::MAX,
            u64::MIN,
            42,
            0xFF00_FF00_FF00_FF00,
            0x1234_5678_90AB_CDEF,
        ] {
            roundtrip_u64(v);
        }
    }

    #[test]
    fn encode_decode_preserves_u128_values() {
        for &v in &[
            0,
            1,
            u128::MAX,
            u128::MIN,
            42,
            0xFFFF_0000_FFFF_0000_FFFF_0000_FFFF_0000,
            0x0123_4567_89AB_CDEF_0123_4567_89AB_CDEF,
        ] {
            roundtrip_u128(v);
        }
    }

    #[test]
    fn decode_accepts_lowercase_characters() {
        let encoded_upper = b"ABCD123";
        let encoded_lower = b"abcd123";
        let val_upper = decode_base32::<u32, ()>(encoded_upper).unwrap();
        let val_lower = decode_base32::<u32, ()>(encoded_lower).unwrap();
        assert_eq!(val_upper, val_lower);
    }

    #[test]
    fn decode_accepts_mixed_case_characters() {
        let encoded_upper = b"ABCD123";
        let encoded_mixed = b"aBcD123";
        let val_upper = decode_base32::<u32, ()>(encoded_upper).unwrap();
        let val_mixed = decode_base32::<u32, ()>(encoded_mixed).unwrap();
        assert_eq!(val_upper, val_mixed);
    }

    #[test]
    fn decode_treats_crockford_aliases_as_canonical_values() {
        // Test that Crockford aliases decode to their canonical values
        let aliases = [
            (b'O', b'0'),
            (b'o', b'0'),
            (b'I', b'1'),
            (b'i', b'1'),
            (b'L', b'1'),
            (b'l', b'1'),
        ];
        for (alias, canonical) in aliases {
            let alias_buf = [alias; 7];
            let canonical_buf = [canonical; 7];
            let alias_val = decode_base32::<u32, ()>(&alias_buf).unwrap();
            let canonical_val = decode_base32::<u32, ()>(&canonical_buf).unwrap();
            assert_eq!(
                alias_val, canonical_val,
                "alias {} should decode to same value as {}",
                alias as char, canonical as char
            );
        }
    }

    #[test]
    fn decode_returns_error_for_invalid_bytes() {
        let test_cases = [
            (b"ZZZZZZ!" as &[u8], b'!', 6), // Invalid ASCII character
            (b"ABCD\xFF23", 0xFF, 4),       // Invalid UTF-8 byte
            (b"ABC\xC3\xA9EF", 0xC3, 3),    // Multi-byte UTF-8 (é)
            (b"ZZZZZ\x80Z", 0x80, 5),       // UTF-8 continuation byte
        ];

        for (input, expected_byte, expected_index) in test_cases {
            let result = decode_base32::<u32, ()>(input);
            assert_eq!(
                result.unwrap_err(),
                Error::DecodeInvalidAscii {
                    byte: expected_byte,
                    index: expected_index,
                },
                "Failed for input with byte 0x{:02X} at index {}",
                expected_byte,
                expected_index
            );
        }
    }
}
