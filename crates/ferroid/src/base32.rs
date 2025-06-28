use crate::{Error, Id, Result};
use core::array::TryFromSliceError;
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
        let arr = bytes.try_into().map_err(Base32Error::from)?;
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
        let arr = bytes.try_into().map_err(Base32Error::from)?;
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
        let arr = bytes.try_into().map_err(Base32Error::from)?;
        Ok(Self::from_be_bytes(arr))
    }
}

/// Extension trait for types that support Crockford Base32 encoding and
/// decoding.
///
/// This trait enables converting IDs (typically backed by primitive integers)
/// to and from fixed-length, lexicographically sortable Base32 strings using
/// the [Crockford Base32](https://www.crockford.com/base32.html) alphabet.
///
/// It relies on the [`BeBytes`] trait for bit-level access to the underlying
/// integer representation, and produces fixed-width ASCII-encoded output
/// suitable for ordered storage (e.g., in databases, file systems, or URLs).
///
/// # Features
///
/// - Zero-allocation encoding support
/// - Fixed-width, lexicographically sortable output
/// - ASCII-safe encoding using Crockford's Base32 alphabet
/// - Fallible decoding with strong validation
pub trait Base32Ext: Id
where
    Self::Ty: BeBytes,
{
    /// Encodes this ID into a fixed-length [`String`] using Crockford Base32.
    ///
    /// The resulting string is guaranteed to be ASCII and lexicographically
    /// sortable.
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(all(feature = "snowflake", feature = "base32"))]
    /// {
    ///     use ferroid::{Base32Ext, SnowflakeTwitterId};
    ///     let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///     let encoded = id.encode();
    ///     assert_eq!(encoded, "46JA7902CV4V4");
    /// }
    /// ```
    fn encode(&self) -> String {
        let mut buf = <Self::Ty as BeBytes>::Base32Array::default();
        self.encode_to_buf(&mut buf);

        // SAFETY: Crockford Base32 output is always valid ASCII
        unsafe { String::from_utf8_unchecked(buf.as_ref().to_vec()) }
    }
    /// Encodes this ID into the provided output buffer without allocating.
    ///
    /// This is the zero-allocation alternative to [`Self::encode`]. The buffer
    /// must be exactly [`BeBytes::BASE32_SIZE`] in length.
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(all(feature = "snowflake", feature = "base32"))]
    /// {
    ///     use ferroid::{Base32Ext, BeBytes, Id, SnowflakeTwitterId};
    ///     let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///     let mut buf = <<SnowflakeTwitterId as Id>::Ty as BeBytes>::Base32Array::default();
    ///     id.encode_to_buf(&mut buf);
    ///
    ///     // SAFETY: Crockford Base32 output is always valid ASCII
    ///     let encoded = unsafe { core::str::from_utf8_unchecked(buf.as_ref()) };
    ///     assert_eq!(encoded, "46JA7902CV4V4");
    /// }
    /// ```
    fn encode_to_buf(&self, buf: &mut <<Self as Id>::Ty as BeBytes>::Base32Array) {
        encode_base32(self.to_raw(), buf);
    }
    /// Decodes a Base32-encoded string back into an ID.
    ///
    /// **Note:** This method performs a structural decode of the Base32 string
    /// into the raw underlying integer. It does **not** validate whether the
    /// decoded value adheres to the ID's semantic constraints (e.g., reserved
    /// bits, out-of-range fields).
    ///
    /// If validation is required, use `.is_valid()` to check, or
    /// `.into_valid()` to normalize the value.
    ///
    /// # Errors
    ///
    /// Returns an error if the input string:
    /// - is not the expected fixed length
    /// - contains characters not in the Crockford Base32 alphabet (invalid
    ///   ASCII)
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(all(feature = "snowflake", feature = "base32"))]
    /// {
    ///     use ferroid::{Base32Ext, Snowflake, SnowflakeTwitterId};
    ///
    ///     // A valid encoded ID
    ///     let encoded = "46JA7902CV4V4";
    ///     let decoded = SnowflakeTwitterId::decode(encoded).unwrap();
    ///
    ///     assert!(decoded.is_valid());
    ///     assert_eq!(decoded.to_raw(), 2_424_242_424_242_424_242);
    ///
    ///     // A syntactically valid but semantically invalid `SnowflakeTwitterId` - sets reserved bits
    ///     let encoded = "ZZZZZZZZZZZZZ";
    ///     let decoded = SnowflakeTwitterId::decode(encoded).unwrap();
    ///
    ///     assert!(!decoded.is_valid());
    ///     assert_eq!(decoded.to_raw(), u64::MAX);
    ///
    ///     // Normalize to a valid representation
    ///     let valid = decoded.into_valid();
    ///     assert!(valid.is_valid());
    ///     assert_eq!(valid.to_raw(), 9_223_372_036_854_775_807); // max valid `SnowflakeTwitterId`
    /// }
    /// ```
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
    DecodeInvalidLen(usize),
    DecodeInvalidAscii(u8),
    TryFromSliceError(core::array::TryFromSliceError),
}
impl fmt::Display for Base32Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Base32Error::DecodeInvalidAscii(b) => write!(f, "invalid ascii byte: {b}"),
            Base32Error::DecodeInvalidLen(len) => write!(f, "invalid length: {len}"),
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

impl From<TryFromSliceError> for Base32Error {
    fn from(err: TryFromSliceError) -> Self {
        Base32Error::TryFromSliceError(err)
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
const BITS_PER_CHAR: usize = 5;

pub fn encode_base32<T: BeBytes>(val: T, buf: &mut T::Base32Array) {
    let mut bits = 0_usize;
    let mut acc = 0_u16;
    let mask = 0x1F_u16;

    let byte_array = val.to_be_bytes();
    let bytes = byte_array.as_ref();
    let mut out = 0;

    let buf_slice = buf.as_mut();

    for &b in bytes {
        acc = (acc << 8) | b as u16;
        bits += 8;

        while bits >= BITS_PER_CHAR && out < buf_slice.len() {
            bits -= BITS_PER_CHAR;
            let index = ((acc >> bits) & mask) as usize;
            buf_slice[out] = ALPHABET[index];
            out += 1;
        }
    }

    // Pad top bits
    if bits > 0 && out < buf_slice.len() {
        let index = ((acc << (BITS_PER_CHAR - bits)) & mask) as usize;
        buf_slice[out] = ALPHABET[index];
    }
}

/// Decodes a fixed-length Crockford base32 string into the primitive integer
/// type.
fn decode_base32<
    T: BeBytes
        + Default
        + From<u8>
        + core::ops::Shl<usize, Output = T>
        + core::ops::Shr<usize, Output = T>
        + core::ops::BitOr<Output = T>,
>(
    encoded: &str,
) -> Result<T> {
    if encoded.len() != T::BASE32_SIZE {
        return Err(Error::Base32Error(Base32Error::DecodeInvalidLen(
            encoded.len(),
        )));
    }
    let mut acc = T::default();
    let total_bits = T::BASE32_SIZE * BITS_PER_CHAR;
    let target_bits = T::SIZE * 8;
    let excess = total_bits.saturating_sub(target_bits);

    for (i, b) in encoded.bytes().enumerate() {
        let val = LOOKUP[b as usize];
        if val == NO_VALUE {
            return Err(Error::Base32Error(Base32Error::DecodeInvalidAscii(b)));
        }

        if excess > 0 && i == T::BASE32_SIZE - 1 {
            // Last character with excess bits: shift by less to avoid overflow
            acc = (acc << (BITS_PER_CHAR - excess)) | (T::from(val) >> excess);
        } else {
            // Normal accumulation
            acc = (acc << BITS_PER_CHAR) | T::from(val);
        }
    }

    Ok(acc)
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
        let mut buf = <T::Ty as BeBytes>::Base32Array::default();
        id.encode_to_buf(&mut buf);
        let encoded = core::str::from_utf8(buf.as_ref()).unwrap();
        let decoded = T::decode(encoded).expect("decode should succeed");

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
            Err(Error::Base32Error(Base32Error::DecodeInvalidAscii(64)))
        ));
    }

    #[test]
    fn decode_invalid_length_fails() {
        // Shorter than 13-byte base32 for u64 (decoded slice won't be 8 bytes)
        let too_short = "012345678901";
        let result = SnowflakeTwitterId::decode(too_short);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidLen(12)))
        ));

        // Longer than 13-byte base32 for u64 (decoded slice won't be 8 bytes)
        let too_long = "01234567890123";
        let result = SnowflakeTwitterId::decode(too_long);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidLen(14)))
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

        let mut buf = <T::Ty as BeBytes>::Base32Array::default();
        id.encode_to_buf(&mut buf);
        let encoded = core::str::from_utf8(buf.as_ref()).unwrap();
        let decoded = T::decode(encoded).expect("decode should succeed");

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
