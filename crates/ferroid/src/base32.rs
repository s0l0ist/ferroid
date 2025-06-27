use crate::{Error, Id, Result};
use core::convert::TryInto;
use core::fmt;

const fn base32_size(bytes: usize) -> usize {
    ((bytes * 8) + 4) / 5
}

/// A trait for types that can be encoded to and decoded from big-endian bytes.
pub trait BeBytes: Sized {
    const SIZE: usize;
    const BASE32_SIZE: usize;
    type ByteArray: AsRef<[u8]> + AsMut<[u8]> + Default + Copy;
    type Base32Array: AsRef<[u8]> + AsMut<[u8]> + Default + Copy;

    fn to_be_bytes(self) -> Self::ByteArray;
    fn from_be_bytes(bytes: &[u8]) -> Result<Self>;
}
impl BeBytes for u32 {
    const SIZE: usize = core::mem::size_of::<u32>();
    const BASE32_SIZE: usize = base32_size(Self::SIZE);

    type ByteArray = [u8; Self::SIZE];
    type Base32Array = [u8; Self::BASE32_SIZE];

    fn to_be_bytes(self) -> Self::ByteArray {
        self.to_be_bytes()
    }

    fn from_be_bytes(bytes: &[u8]) -> Result<Self> {
        let arr: [u8; Self::SIZE] = bytes
            .try_into()
            .map_err(|e| Base32Error::TryFromSliceError(e))?;
        Ok(Self::from_be_bytes(arr))
    }
}
impl BeBytes for u64 {
    const SIZE: usize = core::mem::size_of::<u64>();
    const BASE32_SIZE: usize = base32_size(Self::SIZE);

    type ByteArray = [u8; Self::SIZE];
    type Base32Array = [u8; Self::BASE32_SIZE];

    fn to_be_bytes(self) -> Self::ByteArray {
        self.to_be_bytes()
    }

    fn from_be_bytes(bytes: &[u8]) -> Result<Self> {
        let arr: [u8; Self::SIZE] = bytes
            .try_into()
            .map_err(|e| Base32Error::TryFromSliceError(e))?;
        Ok(Self::from_be_bytes(arr))
    }
}
impl BeBytes for u128 {
    const SIZE: usize = core::mem::size_of::<u128>();
    const BASE32_SIZE: usize = base32_size(Self::SIZE);

    type ByteArray = [u8; Self::SIZE];
    type Base32Array = [u8; Self::BASE32_SIZE];

    fn to_be_bytes(self) -> Self::ByteArray {
        self.to_be_bytes()
    }

    fn from_be_bytes(bytes: &[u8]) -> Result<Self> {
        let arr: [u8; Self::SIZE] = bytes
            .try_into()
            .map_err(|e| Base32Error::TryFromSliceError(e))?;
        Ok(Self::from_be_bytes(arr))
    }
}

/// A trait for types that can be encoded to and decoded from base32 (crockford)
/// strings.
pub trait Base32Ext: Id
where
    Self::Ty: BeBytes,
{
    fn encode(&self) -> String {
        let mut buf = <Self::Ty as BeBytes>::Base32Array::default();
        encode_base32(self.to_raw(), &mut buf);

        // SAFTEY: Base32 is always valid ASCII
        unsafe { String::from_utf8_unchecked(buf.as_ref().to_vec()) }
    }

    fn decode(s: &str) -> Result<Self> {
        let raw = decode_base32(s)?;
        Ok(Self::from_raw(raw))
    }
}

impl<ID> Base32Ext for ID
where
    ID: Id,
    ID::Ty: BeBytes,
{
}

#[derive(Clone, Debug)]
pub enum Base32Error {
    DecodeInvalidLen,
    DecodeInvalidAscii,
    TryFromSliceError(std::array::TryFromSliceError),
}
impl fmt::Display for Base32Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Base32Error::DecodeInvalidAscii => write!(f, "invalid ascii char"),
            Base32Error::DecodeInvalidLen => write!(f, "invalid length"),
            Base32Error::TryFromSliceError(e) => write!(f, "{}", e),
        }
    }
}
impl core::error::Error for Base32Error {}
impl From<Base32Error> for Error {
    fn from(err: Base32Error) -> Self {
        Error::Base32Error(err)
    }
}

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const NO_VALUE: u8 = 255;

/// Lookup table for Crockford base32 decoding
const LOOKUP: [u8; 256] = {
    let mut lut = [NO_VALUE; 256];
    let mut i = 0;
    while i < 32 {
        let c = ALPHABET[i];
        lut[c as usize] = i as u8;
        if c.is_ascii_alphabetic() {
            lut[(c + 32) as usize] = i as u8; // lowercase
        }
        i += 1;
    }
    lut
};

pub fn encode_base32<T: BeBytes>(value: T, buf: &mut T::Base32Array) {
    let mut bits = 0usize;
    let mut acc = 0u16;

    let raw = value.to_be_bytes();
    let bytes = raw.as_ref();
    let mut out = 0;

    let buf_slice = buf.as_mut();

    for &b in bytes {
        acc = (acc << 8) | b as u16;
        bits += 8;

        while bits >= 5 && out < buf_slice.len() {
            bits -= 5;
            let index = ((acc >> bits) & 0x1F) as usize;
            buf_slice[out] = ALPHABET[index];
            out += 1;
        }
    }

    // Padding: top bits if any
    if bits > 0 && out < buf_slice.len() {
        let index = ((acc << (5 - bits)) & 0x1F) as usize;
        buf_slice[out] = ALPHABET[index];
    }
}

/// Decodes a fixed-length Crockford base32 string into the primitive integer type.
fn decode_base32<T: BeBytes>(s: &str) -> Result<T> {
    if s.len() != T::BASE32_SIZE {
        return Err(Error::Base32Error(Base32Error::DecodeInvalidLen));
    }

    let bytes = s.as_bytes();
    let mut out = T::ByteArray::default();
    let out_bytes = out.as_mut();

    // Reverse the encoding process - accumulate bits and write to bytes
    let mut bits = 0usize;
    let mut acc = 0u16;
    let mut byte_idx = 0;

    for &b in bytes {
        let val = LOOKUP[b as usize];
        if val == NO_VALUE {
            return Err(Error::Base32Error(Base32Error::DecodeInvalidAscii));
        }

        acc = (acc << 5) | val as u16;
        bits += 5;

        // Extract complete bytes
        while bits >= 8 && byte_idx < out_bytes.len() {
            bits -= 8;
            out_bytes[byte_idx] = (acc >> bits) as u8;
            byte_idx += 1;
        }
    }

    // Handle any remaining bits in the last partial byte
    if bits > 0 && byte_idx < out_bytes.len() {
        // Shift remaining bits to the left to fill the byte
        let remaining_bits = 8 - bits;
        out_bytes[byte_idx] = (acc << remaining_bits) as u8;
    }

    T::from_be_bytes(out_bytes)
}

#[cfg(all(test, feature = "snowflake"))]
mod snowflake_tests {
    use super::*;
    use crate::{
        Snowflake, SnowflakeDiscordId, SnowflakeInstagramId, SnowflakeMastodonId,
        SnowflakeTwitterId,
    };
    use core::{any::type_name, fmt};

    fn test_encode_decode_snowflake<T>(id: T, label: &str)
    where
        T: Snowflake + Base32Ext + PartialEq + fmt::Debug,
        T::Ty: BeBytes + fmt::Binary + fmt::LowerHex + fmt::Display + fmt::Debug,
    {
        let raw = id.to_raw();
        let encoded = id.encode();
        let decoded = T::decode(&encoded).expect("decode should succeed");

        let type_name = type_name::<T>();

        println!("=== {} {} ===", type_name, label);
        println!("id (raw decimal): {}", raw);
        println!("id (raw binary):  {:064b}", raw);
        println!("timestamp:  0x{:x}", id.timestamp());
        println!("machine id: 0x{:x}", id.machine_id());
        println!("sequence:   0x{:x}", id.sequence());
        println!("encoded:    {}", encoded);
        println!("decoded:    {}", decoded);

        assert_eq!(id, decoded, "{} roundtrip failed for {}", label, type_name);
    }

    #[test]
    fn twitter_max() {
        let id = SnowflakeTwitterId::from_components(
            SnowflakeTwitterId::max_timestamp(),
            SnowflakeTwitterId::max_machine_id(),
            SnowflakeTwitterId::max_sequence(),
        );
        test_encode_decode_snowflake(id, "max");
        assert_eq!(id.to_raw(), 9_223_372_036_854_775_807) // 1 bit reserved
    }

    #[test]
    fn twitter_zero() {
        let id = SnowflakeTwitterId::from_components(
            SnowflakeTwitterId::ZERO,
            SnowflakeTwitterId::ZERO,
            SnowflakeTwitterId::ZERO,
        );
        test_encode_decode_snowflake(id, "zero");
        assert_eq!(id.to_raw(), 0)
    }

    #[test]
    fn discord_max() {
        let id = SnowflakeDiscordId::from_components(
            SnowflakeDiscordId::max_timestamp(),
            SnowflakeDiscordId::max_machine_id(),
            SnowflakeDiscordId::max_sequence(),
        );
        test_encode_decode_snowflake(id, "max");
        assert_eq!(id.to_raw(), 18_446_744_073_709_551_615)
    }

    #[test]
    fn discord_zero() {
        let id = SnowflakeDiscordId::from_components(
            SnowflakeDiscordId::ZERO,
            SnowflakeDiscordId::ZERO,
            SnowflakeDiscordId::ZERO,
        );
        test_encode_decode_snowflake(id, "zero");
        assert_eq!(id.to_raw(), 0)
    }

    #[test]
    fn instagram_max() {
        let id = SnowflakeInstagramId::from_components(
            SnowflakeInstagramId::max_timestamp(),
            SnowflakeInstagramId::max_machine_id(),
            SnowflakeInstagramId::max_sequence(),
        );
        test_encode_decode_snowflake(id, "max");
        assert_eq!(id.to_raw(), 18_446_744_073_709_551_615)
    }

    #[test]
    fn instagram_zero() {
        let id = SnowflakeInstagramId::from_components(
            SnowflakeInstagramId::ZERO,
            SnowflakeInstagramId::ZERO,
            SnowflakeInstagramId::ZERO,
        );
        test_encode_decode_snowflake(id, "zero");
        assert_eq!(id.to_raw(), 0)
    }

    #[test]
    fn mastodon_max() {
        let id = SnowflakeMastodonId::from_components(
            SnowflakeMastodonId::max_timestamp(),
            SnowflakeMastodonId::max_machine_id(),
            SnowflakeMastodonId::max_sequence(),
        );
        test_encode_decode_snowflake(id, "max");
        assert_eq!(id.to_raw(), 18_446_744_073_709_551_615)
    }

    #[test]
    fn mastodon_zero() {
        let id = SnowflakeMastodonId::from_components(
            SnowflakeMastodonId::ZERO,
            SnowflakeMastodonId::ZERO,
            SnowflakeMastodonId::ZERO,
        );
        test_encode_decode_snowflake(id, "zero");
        assert_eq!(id.to_raw(), 0)
    }

    #[test]
    fn decode_invalid_character_fails() {
        // Base32 Crockford disallows symbols like `@`
        let invalid = "012345678901@";
        let result = SnowflakeTwitterId::decode(invalid);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidAscii))
        ));
    }

    #[test]
    fn decode_invalid_length_fails() {
        // Shorter than 13-byte base32 for u64 (decoded slice won't be 8 bytes)
        let too_short = "012345678901";
        let result = SnowflakeTwitterId::decode(too_short);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidLen))
        ));

        // Longer than 13-byte base32 for u64 (decoded slice won't be 8 bytes)
        let too_long = "01234567890123";
        let result = SnowflakeTwitterId::decode(too_long);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidLen))
        ));
    }
}

#[cfg(all(test, feature = "ulid"))]
mod ulid_tests {
    use super::*;
    use crate::{ULID, Ulid};
    use core::{any::type_name, fmt};

    fn test_encode_decode_ulid<T>(id: T, label: &str)
    where
        T: Ulid + Base32Ext + PartialEq + fmt::Debug,
        T::Ty: BeBytes + fmt::Binary + fmt::LowerHex + fmt::Display + fmt::Debug,
    {
        let raw = id.to_raw();
        let encoded = id.encode();
        let decoded = T::decode(&encoded).expect("decode should succeed");

        let type_name = type_name::<T>();

        println!("=== {} {} ===", type_name, label);
        println!("id (raw decimal): {}", raw);
        println!("id (raw binary):  {:064b}", raw);
        println!("timestamp:  0x{:x}", id.timestamp());
        println!("random: 0x{:x}", id.random());
        println!("encoded:    {}", encoded);
        println!("decoded:    {}", decoded);

        assert_eq!(id, decoded, "{} roundtrip failed for {}", label, type_name);
    }

    #[test]
    fn ulid_max() {
        let id = ULID::from_components(ULID::max_timestamp(), ULID::max_random());
        test_encode_decode_ulid(id, "max");
        assert_eq!(id.to_raw(), u128::MAX)
    }

    #[test]
    fn ulid_zero() {
        let id = ULID::from_components(0, 0);
        println!("id {:#?}", id);
        test_encode_decode_ulid(id, "zero");
        assert_eq!(id.to_raw(), 0)
    }
}
