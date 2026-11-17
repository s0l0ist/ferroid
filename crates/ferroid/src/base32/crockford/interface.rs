//! Select fixed-width SIMD kernels or the scalar fallback.

use core::ops::{BitOr, Shl};

#[cfg(all(
    feature = "intrinsics",
    target_arch = "aarch64",
    target_feature = "neon"
))]
use super::neon as intrinsic;
#[cfg(all(
    feature = "intrinsics",
    target_arch = "x86_64",
    target_feature = "ssse3"
))]
use super::ssse3 as intrinsic;
use super::{common::validate_symbols, scalar};
use crate::{base32::Error, generator::Result, id::BeBytes};

/// Encodes supported buffer sizes with a native kernel or the scalar fallback.
///
/// On success, every output byte contains a Crockford ASCII character.
#[inline(always)]
#[allow(clippy::inline_always)]
pub(in crate::base32) fn encode_base32<T: BeBytes>(
    input: &T::ByteArray,
    output: &mut T::Base32Array,
) {
    #[cfg(all(
        feature = "intrinsics",
        any(
            all(target_arch = "aarch64", target_feature = "neon"),
            all(target_arch = "x86_64", target_feature = "ssse3"),
        )
    ))]
    {
        if let (Ok(input), Ok(output)) = (input.as_ref().try_into(), output.as_mut().try_into()) {
            // SAFETY:
            // - The cfg guard requires the backend's target feature.
            // - try_into verifies the exact input and output array lengths.
            return unsafe { intrinsic::encode64(input, output) };
        }
        if let (Ok(input), Ok(output)) = (input.as_ref().try_into(), output.as_mut().try_into()) {
            // SAFETY:
            // - The cfg guard requires the backend's target feature.
            // - try_into verifies the exact input and output array lengths.
            return unsafe { intrinsic::encode128(input, output) };
        }
    }
    scalar::encode_base32::<T>(input, output);
}

/// Decodes the backing value using a native kernel or the scalar fallback.
///
/// The public Base32 interface checks the input length before dispatch.
#[inline(always)]
#[allow(clippy::inline_always)]
pub(in crate::base32) fn decode_base32<T, ID>(bytes: &[u8]) -> Result<T, Error<ID>>
where
    T: BeBytes + Default + From<u8> + Shl<usize, Output = T> + BitOr<Output = T>,
{
    let (decoded, has_error) = core::cfg_select! {
        all(
            feature = "intrinsics",
            any(
                all(target_arch = "aarch64", target_feature = "neon"),
                all(target_arch = "x86_64", target_feature = "ssse3"),
            )
        ) => {
            if (T::SIZE, T::BASE32_SIZE) == (u64::SIZE, u64::BASE32_SIZE)
                && let Ok(input) = bytes.try_into()
            {
                // SAFETY:
                // - The cfg guard requires the backend's target feature.
                // - try_into verifies the exact input array length; the kernel
                //   bounds-checks the output slice before its vector store.
                unsafe { intrinsic::decode64::<T>(input) }
            } else if (T::SIZE, T::BASE32_SIZE) == (u128::SIZE, u128::BASE32_SIZE)
                && let Ok(input) = bytes.try_into()
            {
                // SAFETY:
                // - The cfg guard requires the backend's target feature.
                // - try_into verifies the exact input array length; the kernel
                //   bounds-checks the output slice before its vector store.
                unsafe { intrinsic::decode128::<T>(input) }
            } else {
                scalar::decode_base32::<T>(bytes)
            }
        }
        _ => { scalar::decode_base32::<T>(bytes) }
    };
    if has_error {
        validate_symbols(bytes)?;
    }
    Ok(decoded)
}

#[cfg(test)]
mod tests {
    use core::fmt::Debug;

    use super::{decode_base32, encode_base32};
    use crate::{base32::Error, id::BeBytes};

    const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

    fn encode<T: BeBytes>(value: T) -> T::Base32Array {
        let mut encoded = T::Base32Array::default();
        encode_base32::<T>(&value.to_be_bytes(), &mut encoded);
        encoded
    }

    fn assert_roundtrip<T>(value: T)
    where
        T: BeBytes
            + Copy
            + Debug
            + Eq
            + Default
            + From<u8>
            + core::ops::Shl<usize, Output = T>
            + core::ops::BitOr<Output = T>,
    {
        let encoded = encode(value);
        assert_eq!(decode_base32::<T, ()>(encoded.as_ref()), Ok(value));
    }

    #[test]
    fn encode_u32_has_fixed_width_and_leading_zero_padding() {
        assert_eq!(&encode(0_u32), b"0000000");
        assert_eq!(&encode(42_u32), b"000001A");
        assert_eq!(&encode(u32::MAX), b"3ZZZZZZ");
    }

    #[test]
    fn encode_u64_has_fixed_width_and_leading_zero_padding() {
        assert_eq!(&encode(0_u64), b"0000000000000");
        assert_eq!(&encode(42_u64), b"000000000001A");
        assert_eq!(&encode(u64::MAX), b"FZZZZZZZZZZZZ");
    }

    #[test]
    fn encode_u128_has_fixed_width_and_leading_zero_padding() {
        assert_eq!(&encode(0_u128), b"00000000000000000000000000");
        assert_eq!(&encode(42_u128), b"0000000000000000000000001A");
        assert_eq!(&encode(u128::MAX), b"7ZZZZZZZZZZZZZZZZZZZZZZZZZ");
    }

    #[test]
    fn encode_decode_preserves_u32_values() {
        for value in [0, 1, u32::MAX, 42, 0xFF00_FF00, 0x1234_5678] {
            assert_roundtrip(value);
        }
    }

    #[test]
    fn encode_decode_preserves_u64_values() {
        for value in [
            0,
            1,
            u64::MAX,
            42,
            0xFF00_FF00_FF00_FF00,
            0x1234_5678_90AB_CDEF,
        ] {
            assert_roundtrip(value);
        }
    }

    #[test]
    fn encode_decode_preserves_u128_values() {
        for value in [
            0,
            1,
            u128::MAX,
            42,
            0xFFFF_0000_FFFF_0000_FFFF_0000_FFFF_0000,
            0x0123_4567_89AB_CDEF_0123_4567_89AB_CDEF,
        ] {
            assert_roundtrip(value);
        }
    }

    #[test]
    fn roundtrip_preserves_each_bit_and_its_complement() {
        for bit in 0..u32::BITS {
            assert_roundtrip(1_u32 << bit);
            assert_roundtrip(!(1_u32 << bit));
        }
        for bit in 0..u64::BITS {
            assert_roundtrip(1_u64 << bit);
            assert_roundtrip(!(1_u64 << bit));
        }
        for bit in 0..u128::BITS {
            assert_roundtrip(1_u128 << bit);
            assert_roundtrip(!(1_u128 << bit));
        }
    }

    #[test]
    fn roundtrip_preserves_sampled_values() {
        // Fixed seed: cover varied bit combinations without an RNG dependency.
        let mut value = 0x0123_4567_89AB_CDEF_FEDC_BA98_7654_3210_u128;
        for _ in 0..10_000 {
            value ^= value << 23;
            value ^= value >> 17;
            value ^= value << 26;
            let bytes = value.to_le_bytes();
            assert_roundtrip(u32::from_le_bytes(bytes[..4].try_into().unwrap()));
            assert_roundtrip(u64::from_le_bytes(bytes[..8].try_into().unwrap()));
            assert_roundtrip(value);
        }
    }

    #[test]
    fn decode_accepts_lowercase_characters() {
        assert_eq!(
            decode_base32::<u32, ()>(b"abcd123"),
            decode_base32(b"ABCD123")
        );
        assert_eq!(
            decode_base32::<u64, ()>(b"abcd123456789"),
            decode_base32(b"ABCD123456789")
        );
        assert_eq!(
            decode_base32::<u128, ()>(b"01arz3ndektsv4rrffq69g5fav"),
            decode_base32(b"01ARZ3NDEKTSV4RRFFQ69G5FAV")
        );
    }

    #[test]
    fn decode_accepts_mixed_case_characters() {
        assert_eq!(
            decode_base32::<u32, ()>(b"aBcD123"),
            decode_base32(b"ABCD123")
        );
        assert_eq!(
            decode_base32::<u64, ()>(b"aBcD123456789"),
            decode_base32(b"ABCD123456789")
        );
        assert_eq!(
            decode_base32::<u128, ()>(b"01aRz3NdEkTsV4rRfFq69g5FaV"),
            decode_base32(b"01ARZ3NDEKTSV4RRFFQ69G5FAV")
        );
    }

    #[test]
    fn decode_treats_crockford_aliases_as_canonical_values() {
        for (alias, canonical) in [
            (b'O', b'0'),
            (b'o', b'0'),
            (b'I', b'1'),
            (b'i', b'1'),
            (b'L', b'1'),
            (b'l', b'1'),
        ] {
            assert_eq!(
                decode_base32::<u32, ()>(&[alias; 7]),
                decode_base32(&[canonical; 7])
            );
            assert_eq!(
                decode_base32::<u64, ()>(&[alias; 13]),
                decode_base32(&[canonical; 13])
            );
            assert_eq!(
                decode_base32::<u128, ()>(&[alias; 26]),
                decode_base32(&[canonical; 26])
            );
        }
    }

    #[test]
    fn decode_discards_excess_high_bits_and_reencodes_canonically() {
        let raw32 = decode_base32::<u32, ()>(b"ZZZZZZZ").unwrap();
        let raw64 = decode_base32::<u64, ()>(b"ZZZZZZZZZZZZZ").unwrap();
        let raw128 = decode_base32::<u128, ()>(b"ZZZZZZZZZZZZZZZZZZZZZZZZZZ").unwrap();
        assert_eq!(raw32, u32::MAX);
        assert_eq!(raw64, u64::MAX);
        assert_eq!(raw128, u128::MAX);
        assert_eq!(&encode(raw32), b"3ZZZZZZ");
        assert_eq!(&encode(raw64), b"FZZZZZZZZZZZZ");
        assert_eq!(&encode(raw128), b"7ZZZZZZZZZZZZZZZZZZZZZZZZZ");
    }

    #[test]
    fn decode_rejects_invalid_ascii_characters() {
        for byte in [b'!', b'@', b'U', b'u', b'=', b' ', 0] {
            let mut input = *b"0000000";
            input[3] = byte;
            assert_eq!(
                decode_base32::<u32, ()>(&input),
                Err(Error::DecodeInvalidAscii { byte, index: 3 })
            );
        }
    }

    #[test]
    fn decode_rejects_non_ascii_and_utf8_sequences() {
        for (input, byte, index) in [
            (b"ABCD\xFF23", 0xFF, 4),
            (b"ABC\xC3\xA9EF", 0xC3, 3),
            (b"ZZZZZ\x80Z", 0x80, 5),
        ] {
            assert_eq!(
                decode_base32::<u32, ()>(input),
                Err(Error::DecodeInvalidAscii { byte, index })
            );
        }
    }

    fn assert_every_byte_at_every_position<T>()
    where
        T: BeBytes
            + Debug
            + Eq
            + Default
            + From<u8>
            + core::ops::Shl<usize, Output = T>
            + core::ops::BitOr<Output = T>,
    {
        let mut input = T::Base32Array::default();
        input.as_mut().fill(b'0');
        for index in 0..T::BASE32_SIZE {
            for byte in 0..=u8::MAX {
                input.as_mut()[index] = byte;
                // Use an independent alphabet lookup to check the production table.
                let symbol = match byte.to_ascii_uppercase() {
                    b'O' => Some(0),
                    b'I' | b'L' => Some(1),
                    upper => ALPHABET
                        .iter()
                        .position(|&c| c == upper)
                        .map(|value| u8::try_from(value).unwrap()),
                };
                let expected = match symbol {
                    Some(value) => Ok(T::from(value) << ((T::BASE32_SIZE - 1 - index) * 5)),
                    None => Err(Error::DecodeInvalidAscii { byte, index }),
                };
                assert_eq!(
                    decode_base32::<T, ()>(input.as_ref()),
                    expected,
                    "byte {byte:#04x} at index {index}"
                );
            }
            input.as_mut()[index] = b'0';
        }
    }

    #[test]
    fn decode_u32_checks_every_byte_at_every_position() {
        assert_every_byte_at_every_position::<u32>();
    }

    #[test]
    fn decode_u64_checks_every_byte_at_every_position() {
        assert_every_byte_at_every_position::<u64>();
    }

    #[test]
    fn decode_u128_checks_every_byte_at_every_position() {
        assert_every_byte_at_every_position::<u128>();
    }

    #[test]
    fn decode_reports_first_invalid_byte_across_overlapping_windows() {
        let mut input64 = [b'Z'; 13];
        input64[12] = b'U';
        input64[5] = b'!';
        assert_eq!(
            decode_base32::<u64, ()>(&input64),
            Err(Error::DecodeInvalidAscii {
                byte: b'!',
                index: 5
            })
        );

        let mut input128 = [b'Z'; 26];
        input128[23] = 255;
        input128[17] = b'U';
        assert_eq!(
            decode_base32::<u128, ()>(&input128),
            Err(Error::DecodeInvalidAscii {
                byte: b'U',
                index: 17
            })
        );
        input128[3] = b'!';
        assert_eq!(
            decode_base32::<u128, ()>(&input128),
            Err(Error::DecodeInvalidAscii {
                byte: b'!',
                index: 3
            })
        );
    }

    #[test]
    fn encode_decode_support_exact_sized_unaligned_buffers() {
        fn check<T: Copy + Debug + Eq, const BYTES: usize, const CHARS: usize>(
            encode: fn(&[u8; BYTES], &mut [u8; CHARS]),
            decode: fn(&[u8]) -> Result<T, Error<()>>,
            expected: T,
        ) {
            let input = [255; 64];
            let mut canonical = [0; CHARS];
            encode(&[255; BYTES], &mut canonical);
            for offset in 0..16 {
                let mut text = [0xCC; 64];
                encode(
                    input[offset..offset + BYTES].try_into().unwrap(),
                    (&mut text[offset..offset + CHARS]).try_into().unwrap(),
                );
                assert_eq!(&text[offset..offset + CHARS], &canonical);
                assert!(text[..offset].iter().all(|&byte| byte == 0xCC));
                assert!(text[offset + CHARS..].iter().all(|&byte| byte == 0xCC));
                assert_eq!(decode(&text[offset..offset + CHARS]), Ok(expected));
            }
        }
        check(encode_base32::<u32>, decode_base32::<u32, ()>, u32::MAX);
        check(encode_base32::<u64>, decode_base32::<u64, ()>, u64::MAX);
        check(encode_base32::<u128>, decode_base32::<u128, ()>, u128::MAX);
    }
}
