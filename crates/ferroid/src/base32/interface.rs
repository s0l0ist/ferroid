use crate::{
    base32::{Base32Formatter, Base32FormatterRef, Error},
    id::{BeBytes, Id},
};

/// Crockford Base32 encoding and validated decoding for IDs.
///
/// Automatically implemented for every [`Id`] backed by `u32`, `u64`, or `u128`.
///
/// # Example
///
/// ```
/// # #[cfg(feature = "ulid")]
/// # {
/// use ferroid::{base32::Base32Ext, id::ULID};
///
/// let id = ULID::from_raw(42);
/// assert_eq!(ULID::decode(id.encode().as_str()).unwrap(), id);
///
/// let mut buffer = ULID::base32_array();
/// assert_eq!(id.encode_to_buf(&mut buffer).as_str(), id.encode().as_str());
/// # }
/// ```
pub trait Base32Ext: Id
where
    Self::Ty: BeBytes,
{
    /// Returns the backing integer's default byte buffer.
    #[must_use]
    fn byte_array() -> <Self::Ty as BeBytes>::ByteArray {
        <Self::Ty as BeBytes>::ByteArray::default()
    }

    /// Returns the default fixed-size Base32 buffer.
    #[must_use]
    fn base32_array() -> <Self::Ty as BeBytes>::Base32Array {
        <Self::Ty as BeBytes>::Base32Array::default()
    }

    /// Encodes this ID into a formatter with a stack-allocated buffer.
    #[must_use]
    fn encode(&self) -> Base32Formatter<Self> {
        Base32Formatter::new(self)
    }

    /// Encodes into a caller-supplied buffer and returns a formatter view.
    ///
    /// Use [`Self::base32_array`] to create a buffer of the correct size.
    fn encode_to_buf<'buf>(
        &self,
        buffer: &'buf mut <Self::Ty as BeBytes>::Base32Array,
    ) -> Base32FormatterRef<'buf, Self> {
        Base32FormatterRef::new(self, buffer)
    }

    /// Decodes Base32 and validates the resulting ID through [`TryFrom`].
    ///
    /// Bits beyond the backing integer's width are discarded before validation.
    ///
    /// # Errors
    ///
    /// Returns an error for incorrect length, invalid characters, or an invalid
    /// ID layout, such as reserved bits being set.
    fn decode(input: impl AsRef<[u8]>) -> Result<Self, Error<Self>>
    where
        Self: TryFrom<<Self as Id>::Ty, Error = Error<Self>>,
    {
        let bytes = input.as_ref();
        if bytes.len() != Self::Ty::BASE32_SIZE {
            return Err(Error::DecodeInvalidLen { len: bytes.len() });
        }
        Self::try_from(super::decode_base32(bytes)?)
    }
}

impl<ID> Base32Ext for ID
where
    ID: Id,
    ID::Ty: BeBytes,
{
}

#[cfg(all(test, any(feature = "ulid", feature = "snowflake")))]
mod tests {
    use alloc::string::ToString;

    use crate::{
        base32::{Base32Ext, Error},
        id::{BeBytes, Id},
    };

    fn check_roundtrip<ID>(id: ID, expected: &str)
    where
        ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
        ID::Ty: BeBytes,
    {
        let owned = id.encode();
        assert_eq!(owned.as_bytes(), expected.as_bytes(), "{id:?}");
        assert_eq!(owned.as_str(), expected);
        assert_eq!(owned.clone().as_str(), expected);
        assert_eq!(owned.to_string(), expected);
        assert_eq!(ID::decode(expected), Ok(id));

        // Non-ASCII contents expose any bytes left unwritten by the encoder.
        let mut buffer = ID::base32_array();
        buffer.as_mut().fill(0xFF);
        let borrowed = id.encode_to_buf(&mut buffer);
        assert_eq!(borrowed.as_bytes(), expected.as_bytes());
        assert_eq!(borrowed.as_str(), expected);
        assert_eq!(borrowed.to_string(), expected);

        #[cfg(feature = "alloc")]
        {
            assert_eq!(owned.as_string(), expected);
            assert_eq!(borrowed.as_string(), expected);
        }
    }

    fn check_decode_errors<ID>()
    where
        ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
        ID::Ty: BeBytes,
    {
        let width = ID::Ty::BASE32_SIZE;
        let mut input = alloc::vec![b'0'; width + 1];
        for len in [0, width - 1, width + 1] {
            assert_eq!(
                ID::decode(&input[..len]),
                Err(Error::DecodeInvalidLen { len })
            );
        }
        let index = width / 2;
        input[index] = b'@';
        assert_eq!(
            ID::decode(&input[..width]),
            Err(Error::DecodeInvalidAscii { byte: b'@', index })
        );
    }

    #[cfg(feature = "ulid")]
    mod ulid {
        use super::{check_decode_errors, check_roundtrip};
        use crate::{base32::Base32Ext, id::ULID};

        #[test]
        fn roundtrip_known_values() {
            for (id, expected) in [
                (ULID::from_components(0, 0), "00000000000000000000000000"),
                (
                    ULID::from_components(ULID::max_timestamp(), ULID::max_random()),
                    "7ZZZZZZZZZZZZZZZZZZZZZZZZZ",
                ),
                (
                    ULID::from_components(1_469_922_850_259, 1_012_768_647_078_601_740_696_923),
                    "01ARZ3NDEKTSV4RRFFQ69G5FAV",
                ),
                (
                    ULID::from_components(1_611_559_180_765, 885_339_478_614_498_720_052_741),
                    "01EWW6K6EXQDX5JV0E9CAHPXG5",
                ),
            ] {
                check_roundtrip(id, expected);
            }
        }

        #[test]
        fn string_conversions() {
            let id = ULID::from_raw(42);
            let encoded = "0000000000000000000000001A";
            assert_eq!(alloc::format!("{id}"), encoded);
            assert_eq!(encoded.parse::<ULID>(), Ok(id));
            assert_eq!(ULID::try_from(encoded), Ok(id));
        }

        #[test]
        fn decode_validation() {
            check_decode_errors::<ULID>();
            assert_eq!(
                ULID::decode("ZZZZZZZZZZZZZZZZZZZZZZZZZZ"),
                Ok(ULID::from_raw(u128::MAX))
            );
        }
    }

    #[cfg(feature = "snowflake")]
    mod snowflake {
        use super::{check_decode_errors, check_roundtrip};
        use crate::{
            base32::{Base32Ext, Error},
            id::{
                SnowflakeDiscordId, SnowflakeId, SnowflakeInstagramId, SnowflakeMastodonId,
                SnowflakeTwitterId,
            },
        };

        #[test]
        fn builtin_layouts_roundtrip() {
            fn check<ID>(max_encoded: &str)
            where
                ID: SnowflakeId<Ty = u64> + TryFrom<u64, Error = Error<ID>>,
            {
                check_roundtrip(ID::from_components(0, 0, 0), "0000000000000");
                check_roundtrip(
                    ID::from_components(
                        ID::max_timestamp(),
                        ID::max_machine_id(),
                        ID::max_sequence(),
                    ),
                    max_encoded,
                );
            }

            check::<SnowflakeTwitterId>("7ZZZZZZZZZZZZ");
            check::<SnowflakeDiscordId>("FZZZZZZZZZZZZ");
            check::<SnowflakeInstagramId>("FZZZZZZZZZZZZ");
            check::<SnowflakeMastodonId>("FZZZZZZZZZZZZ");
        }

        #[test]
        #[allow(
            dead_code,
            clippy::wrong_self_convention,
            reason = "The layout fixture generates a complete ID API."
        )]
        fn custom_u32_roundtrip() {
            crate::define_snowflake_id!(SmallId, u32,
                reserved: 0, timestamp: 32, machine_id: 0, sequence: 0);
            check_roundtrip(SmallId::from_raw(42), "000001A");
            check_roundtrip(SmallId::from_raw(u32::MAX), "3ZZZZZZ");
        }

        #[test]
        fn string_conversions() {
            let id = SnowflakeTwitterId::from_raw(42);
            let encoded = "000000000001A";
            assert_eq!(alloc::format!("{id}"), encoded);
            assert_eq!(encoded.parse::<SnowflakeTwitterId>(), Ok(id));
            assert_eq!(SnowflakeTwitterId::try_from(encoded), Ok(id));
        }

        #[test]
        fn decode_validation() {
            check_decode_errors::<SnowflakeTwitterId>();
            let invalid = SnowflakeTwitterId::from_raw(u64::MAX);
            assert_eq!(invalid.encode().as_str(), "FZZZZZZZZZZZZ");
            for encoded in ["FZZZZZZZZZZZZ", "ZZZZZZZZZZZZZ"] {
                let expected = Err(Error::DecodeOverflow { id: invalid });
                assert_eq!(SnowflakeTwitterId::decode(encoded), expected);
                assert_eq!(encoded.parse::<SnowflakeTwitterId>(), expected);
                assert_eq!(SnowflakeTwitterId::try_from(encoded), expected);
            }
        }
    }
}
