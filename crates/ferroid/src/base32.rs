use crate::{Error, Id, Result};
use base32::{Alphabet, decode, encode};
use core::convert::TryInto;

const U32_SIZE: usize = core::mem::size_of::<u32>();
const U64_SIZE: usize = core::mem::size_of::<u64>();
const U128_SIZE: usize = core::mem::size_of::<u128>();

/// A trait for types that can be encoded to and decoded from big-endian bytes.
pub trait BeBytes: Sized {
    type ByteArray: AsRef<[u8]>;

    fn to_be_bytes(self) -> Self::ByteArray;

    fn from_be_bytes(bytes: &[u8]) -> Option<Self>;
}

impl BeBytes for u32 {
    type ByteArray = [u8; U32_SIZE];

    fn to_be_bytes(self) -> Self::ByteArray {
        self.to_be_bytes()
    }

    fn from_be_bytes(bytes: &[u8]) -> Option<Self> {
        let arr: [u8; U32_SIZE] = bytes.try_into().ok()?;
        Some(Self::from_be_bytes(arr))
    }
}

impl BeBytes for u64 {
    type ByteArray = [u8; U64_SIZE];

    fn to_be_bytes(self) -> Self::ByteArray {
        self.to_be_bytes()
    }

    fn from_be_bytes(bytes: &[u8]) -> Option<Self> {
        let arr: [u8; U64_SIZE] = bytes.try_into().ok()?;
        Some(Self::from_be_bytes(arr))
    }
}

impl BeBytes for u128 {
    type ByteArray = [u8; U128_SIZE];

    fn to_be_bytes(self) -> Self::ByteArray {
        self.to_be_bytes()
    }

    fn from_be_bytes(bytes: &[u8]) -> Option<Self> {
        let arr: [u8; U128_SIZE] = bytes.try_into().ok()?;
        Some(Self::from_be_bytes(arr))
    }
}

/// A trait for types that can be encoded to and decoded from base32 (crockford)
/// strings.
pub trait Base32Ext: Id
where
    Self::Ty: BeBytes,
{
    fn encode(&self) -> String {
        let bytes = self.to_raw().to_be_bytes();
        encode(Alphabet::Crockford, bytes.as_ref())
    }

    fn decode(s: &str) -> Result<Self> {
        let bytes = decode(Alphabet::Crockford, s).ok_or(Error::DecodeNonAsciiValue)?;
        let raw = Self::Ty::from_be_bytes(&bytes).ok_or(Error::DecodeInvalidLen)?;
        Ok(Self::from_raw(raw))
    }
}

impl<ID> Base32Ext for ID
where
    ID: Id,
    ID::Ty: BeBytes,
{
}

#[cfg(test)]
mod tests {
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
        let invalid = "01234@6789ABCDEF";
        let result = SnowflakeTwitterId::decode(invalid);
        assert!(matches!(result, Err(Error::DecodeNonAsciiValue)));
    }

    #[test]
    fn decode_invalid_length_fails() {
        // Shorter than 13-byte base32 for u64 (decoded slice won't be 8 bytes)
        let too_short = "ABC";
        let result = SnowflakeTwitterId::decode(too_short);
        assert!(matches!(result, Err(Error::DecodeInvalidLen)));
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
        println!("randomness id: 0x{:x}", id.randomness());
        println!("encoded:    {}", encoded);
        println!("decoded:    {}", decoded);

        assert_eq!(id, decoded, "{} roundtrip failed for {}", label, type_name);
    }

    #[test]
    fn ulid_max() {
        let id = ULID::from_components(ULID::max_timestamp(), ULID::max_randomness());
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
