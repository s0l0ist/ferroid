use crate::{Base32Error, BeBytes, Error, Result};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const NO_VALUE: u8 = 255;
const BITS_PER_CHAR: usize = 5;

/// Lookup table for Crockford base32 decoding
const LOOKUP: [u8; 256] = {
    let mut lut = [NO_VALUE; 256];
    let mut i = 0;
    // Main alphabet, allow lower-case
    while i < 32 {
        let c = ALPHABET[i];
        lut[c as usize] = i as u8;
        if c.is_ascii_uppercase() {
            lut[(c + 32) as usize] = i as u8; // lowercase letter
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

/// Encodes a byte slice into base32, writing output to `buf_slice`.
///
/// # Safety
/// This function assumes `buf_slice` is sized exactly for the base32 output
/// (typically ensured at compile time for encoding primitive types). No
/// undefined behavior will occur if this contract is upheld.
///
/// The internal accumulator (`acc`) is a `u16` and never overflows: bits are
/// always drained in 5-bit groups as soon as possible, so `acc` never exceeds
/// 16 bits.
#[inline(always)]
pub(crate) fn encode_base32(input: &[u8], buf_slice: &mut [u8]) {
    let input_bits = input.len() * 8;
    let output_chars = buf_slice.len();
    let total_bits = output_chars * BITS_PER_CHAR;
    let mut bits = total_bits - input_bits;
    let mut acc = 0_u16;
    let mask = 0x1F;

    let mut out = 0;
    for &b in input {
        acc = (acc << 8) | b as u16;
        bits += 8;
        while bits >= BITS_PER_CHAR {
            bits -= BITS_PER_CHAR;
            unsafe {
                *buf_slice.get_unchecked_mut(out) = ALPHABET[((acc >> bits) & mask) as usize];
            }
            out += 1;
        }
    }
    debug_assert!(bits == 0, "No leftover bits for encoding!");
}

/// Decodes a fixed-length Crockford base32 string into the given primitive
/// integer type.
///
/// Returns an error if the input contains invalid base32 characters. The
/// accumulator never overflows as long as `encoded` fits within the bit width
/// of `T` which the callee must uphold.
#[inline(always)]
pub(crate) fn decode_base32<T>(encoded: &str) -> Result<T>
where
    T: BeBytes
        + Default
        + From<u8>
        + core::ops::Shl<usize, Output = T>
        + core::ops::BitOr<Output = T>,
{
    let mut acc = T::default();
    for b in encoded.bytes() {
        let val = LOOKUP[b as usize];
        if val == NO_VALUE {
            return Err(Error::Base32Error(Base32Error::DecodeInvalidAscii(b)));
        }
        acc = (acc << BITS_PER_CHAR) | T::from(val);
    }

    Ok(acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_u32(val: u32) {
        let bytes = val.to_be_bytes();
        let mut buf = [0u8; 7]; // ceil(32/5) = 7 chars for u32
        encode_base32(&bytes, &mut buf);
        let s = std::str::from_utf8(&buf).unwrap();
        let decoded = decode_base32(s).unwrap();
        assert_eq!(val, decoded, "roundtrip for u32: input={val}, b32={s}");
    }

    fn roundtrip_u64(val: u64) {
        let bytes = val.to_be_bytes();
        let mut buf = [0u8; 13]; // ceil(64/5) = 13 chars for u64
        encode_base32(&bytes, &mut buf);
        let s = std::str::from_utf8(&buf).unwrap();
        let decoded = decode_base32(s).unwrap();
        assert_eq!(val, decoded, "roundtrip for u64: input={val}, b32={s}");
    }

    fn roundtrip_u128(val: u128) {
        let bytes = val.to_be_bytes();
        let mut buf = [0u8; 26]; // ceil(128/5) = 26 chars for u128
        encode_base32(&bytes, &mut buf);
        let s = std::str::from_utf8(&buf).unwrap();
        let decoded = decode_base32(s).unwrap();
        assert_eq!(val, decoded, "roundtrip for u128: input={val}, b32={s}");
    }

    #[test]
    fn test_roundtrip_u32() {
        for &v in &[0, 1, u32::MAX, u32::MIN, 42, 0xFF00FF00, 0x12345678] {
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
            0xFF00FF00FF00FF00,
            0x1234567890ABCDEF,
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
            0xFFFF0000FFFF0000FFFF0000FFFF0000,
            0x0123456789ABCDEF0123456789ABCDEFu128,
        ] {
            roundtrip_u128(v);
        }
    }

    #[test]
    fn test_alias_and_case_insensitive() {
        // Crockford alias: O=o=0, I=i=L=l=1
        let ex = "OILoil";
        for c in ex.bytes() {
            let s = format!("{c:0>7}"); // pad to 7 chars
            let res = decode_base32::<u32>(&s);
            assert!(res.is_ok(), "alias '{c}' failed");
        }
        // Mixed case
        let encoded = "ABCD123";
        let lower = encoded.to_lowercase();
        let upper = encoded.to_uppercase();
        let mid = "aBcD123";
        let val_lower = decode_base32::<u32>(&lower).unwrap();
        let val_upper = decode_base32::<u32>(&upper).unwrap();
        let val_mid = decode_base32::<u32>(mid).unwrap();
        assert_eq!(val_lower, val_upper);
        assert_eq!(val_lower, val_mid);
    }

    #[test]
    fn test_invalid_character() {
        let s = "ZZZZZZ!"; // '!' is not valid
        let res = decode_base32::<u32>(s);
        assert!(res.is_err());
        match res.unwrap_err() {
            Error::Base32Error(Base32Error::DecodeInvalidAscii(b'!')) => {}
            e => panic!("unexpected error: {e:?}"),
        }
    }

    #[test]
    fn test_ulid_ts() {
        let time: u128 = 1469922850259;
        let time_bytes = time.to_be_bytes();
        let mut out = [0u8; 26];
        encode_base32(&time_bytes, &mut out);
        let s = std::str::from_utf8(&out).unwrap();
        // check we can decode it back
        let decoded = decode_base32::<u128>(s).unwrap();
        assert_eq!(decoded, time);
    }
}
