use crate::{Result, base32::Error, id::BeBytes};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const NO_VALUE: u8 = 255;
const BITS_PER_CHAR: usize = 5;

/// Lookup table for Crockford base32 decoding
const LOOKUP: [u8; 256] = {
    let mut lut = [NO_VALUE; 256];
    let mut i = 0_u8;
    // Main alphabet, allow lower-case
    while i < 32 {
        let c = ALPHABET[i as usize];
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
/// - The index into `ALPHABET` is masked with `0x1F` (31), ensuring it is
///   always in the range 0..=31.
///   - `ALPHABET` is a fixed-size array with exactly 32 elements, so all masked
///     indices are valid.
///   - Therefore, `ALPHABET[(acc >> bits) & 0x1F]` is guaranteed to be
///     in-bounds.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn encode_base32(input: &[u8], buf_slice: &mut [u8]) {
    let input_bits = input.len() * 8;
    let output_chars = buf_slice.len();
    let total_bits = output_chars * BITS_PER_CHAR;
    let mut bits = total_bits - input_bits;
    let mut acc = 0_u16;
    let mask = 0x1F;

    let mut out = 0;
    for &b in input {
        acc = (acc << 8) | u16::from(b);
        bits += 8;
        while bits >= BITS_PER_CHAR {
            bits -= BITS_PER_CHAR;
            // SAFETY: `out` is strictly less than `buf_slice.len()`, by
            // caller's contract.
            //
            // SAFETY: `(acc >> bits) & mask` produces values in the range
            // 0..=31.
            unsafe {
                *buf_slice.get_unchecked_mut(out) =
                    *ALPHABET.get_unchecked(((acc >> bits) & mask) as usize);
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
/// - `LOOKUP` is a fixed-size array of 256 elements, so `LOOKUP[b as usize]` is
///   always in-bounds.
#[inline(always)]
#[allow(clippy::inline_always)]
pub fn decode_base32<T, E>(encoded: &str) -> Result<T, Error<E>>
where
    T: BeBytes
        + Default
        + From<u8>
        + core::ops::Shl<usize, Output = T>
        + core::ops::BitOr<Output = T>,
{
    let mut acc = T::default();
    for (i, b) in encoded.bytes().enumerate() {
        // SAFETY: `b as usize` is in 0..=255, and `LOOKUP` has 256 entries.
        let val = unsafe { *LOOKUP.get_unchecked(b as usize) };
        if val == NO_VALUE {
            return Err(Error::DecodeInvalidAscii { byte: b, index: i });
        }
        acc = (acc << BITS_PER_CHAR) | T::from(val);
    }

    Ok(acc)
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    fn roundtrip_u32(val: u32) {
        let bytes = val.to_be_bytes();
        let mut buf = [0u8; 7]; // ceil(32/5) = 7 chars for u32
        encode_base32(&bytes, &mut buf);
        let s = core::str::from_utf8(&buf).unwrap();
        let decoded = decode_base32::<u32, ()>(s).unwrap();
        assert_eq!(val, decoded, "roundtrip for u32: input={val}, b32={s}");
    }

    fn roundtrip_u64(val: u64) {
        let bytes = val.to_be_bytes();
        let mut buf = [0u8; 13]; // ceil(64/5) = 13 chars for u64
        encode_base32(&bytes, &mut buf);
        let s = core::str::from_utf8(&buf).unwrap();
        let decoded = decode_base32::<u64, ()>(s).unwrap();
        assert_eq!(val, decoded, "roundtrip for u64: input={val}, b32={s}");
    }

    fn roundtrip_u128(val: u128) {
        let bytes = val.to_be_bytes();
        let mut buf = [0u8; 26]; // ceil(128/5) = 26 chars for u128
        encode_base32(&bytes, &mut buf);
        let s = core::str::from_utf8(&buf).unwrap();
        let decoded = decode_base32::<u128, ()>(s).unwrap();
        assert_eq!(val, decoded, "roundtrip for u128: input={val}, b32={s}");
    }

    #[test]
    fn test_roundtrip_u32() {
        for &v in &[0, 1, u32::MAX, u32::MIN, 42, 0xFF00_FF00, 0x1234_5678] {
            roundtrip_u32(v);
        }
    }

    #[test]
    fn test_roundtrip_u64() {
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
    fn test_roundtrip_u128() {
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
    fn test_alias_and_case_insensitive() {
        // Crockford alias: O=o=0, I=i=L=l=1
        let ex = "OILoil";
        for c in ex.bytes() {
            let s = std::format!("{c:0>7}"); // pad to 7 chars
            let res = decode_base32::<u32, ()>(&s);
            assert!(res.is_ok(), "alias '{c}' failed");
        }
        // Mixed case
        let encoded = "ABCD123";
        let lower = encoded.to_lowercase();
        let upper = encoded.to_uppercase();
        let mid = "aBcD123";
        let val_lower = decode_base32::<u32, ()>(&lower).unwrap();
        let val_upper = decode_base32::<u32, ()>(&upper).unwrap();
        let val_mid = decode_base32::<u32, ()>(mid).unwrap();
        assert_eq!(val_lower, val_upper);
        assert_eq!(val_lower, val_mid);
    }

    #[test]
    fn test_invalid_character() {
        let s = "ZZZZZZ!"; // '!' is not valid
        let res = decode_base32::<u32, ()>(s);
        assert_eq!(
            res.unwrap_err(),
            Error::DecodeInvalidAscii {
                byte: b'!',
                index: 6,
            }
        );
    }

    #[test]
    fn test_ulid_ts() {
        let time: u128 = 1_469_922_850_259;
        let time_bytes = time.to_be_bytes();
        let mut out = [0u8; 26];
        encode_base32(&time_bytes, &mut out);
        let s = std::str::from_utf8(&out).unwrap();
        // check we can decode it back
        let decoded = decode_base32::<u128, ()>(s).unwrap();
        assert_eq!(decoded, time);
    }
}
