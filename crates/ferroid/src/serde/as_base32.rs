//! Serialize IDs as fixed-width Crockford Base32 strings.

use core::marker::PhantomData;

use serde::{Deserializer, Serializer};

use crate::{
    Error,
    base32::Base32Ext,
    id::{BeBytes, Id},
};

/// Serializes an ID as a Crockford Base32 string using a stack-allocated buffer.
///
/// # Errors
///
/// Returns an error if the serializer fails.
pub fn serialize<ID, S>(id: &ID, serializer: S) -> Result<S::Ok, S::Error>
where
    ID: Id,
    ID::Ty: BeBytes,
    S: Serializer,
{
    serializer.serialize_str(id.encode().as_str())
}

/// Deserializes a Crockford Base32 string and validates the decoded ID.
///
/// Bits beyond the backing integer's width are discarded, matching the public
/// Base32 decoders. The ID must still satisfy [`Id::is_valid`].
///
/// # Errors
///
/// Returns an error for invalid length, characters, or ID layout.
pub fn deserialize<'de, ID, D>(deserializer: D) -> Result<ID, D::Error>
where
    ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
    ID::Ty: BeBytes,
    D: Deserializer<'de>,
{
    struct Base32Visitor<ID>(PhantomData<ID>);

    impl<ID> serde::de::Visitor<'_> for Base32Visitor<ID>
    where
        ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
        ID::Ty: BeBytes,
    {
        type Value = ID;

        fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
            formatter.write_str("a Crockford base32 encoded string")
        }

        #[inline]
        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            ID::decode(value).map_err(E::custom)
        }
    }

    deserializer.deserialize_str(Base32Visitor(PhantomData))
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString};
    use serde::{Deserialize, Serialize};

    use crate::{
        Error,
        id::{BeBytes, Id},
        serde::tests::EvenId,
    };
    #[cfg(feature = "snowflake")]
    use crate::{id::SnowflakeTwitterId, serde::tests::EventId};
    #[cfg(feature = "ulid")]
    use crate::{id::ULID, serde::tests::TraceId};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(bound(
        serialize = "",
        deserialize = "ID: TryFrom<<ID as Id>::Ty, Error = Error<ID>>"
    ))]
    struct Row<ID: Id>
    where
        ID::Ty: BeBytes,
    {
        #[serde(with = "super")]
        id: ID,
    }

    fn check_roundtrip<ID>(id: ID, expected: &str)
    where
        ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
        ID::Ty: BeBytes,
    {
        let row = Row { id };
        let json = serde_json::to_string(&row).unwrap();
        assert_eq!(json, format!("{{\"id\":\"{expected}\"}}"));
        assert_eq!(serde_json::from_str::<Row<ID>>(&json).unwrap(), row);
    }

    fn check_invalid<ID>(json: &str, expected: &str)
    where
        ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
        ID::Ty: BeBytes,
    {
        let error = serde_json::from_str::<Row<ID>>(json).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
    }

    #[test]
    fn roundtrips_builtin_and_custom_ids() {
        check_roundtrip(EvenId(42), "000000000001A");
        #[cfg(feature = "snowflake")]
        {
            check_roundtrip(SnowflakeTwitterId::from_raw(42), "000000000001A");
            check_roundtrip(EventId::from_raw(42), "000001A");
        }
        #[cfg(feature = "ulid")]
        {
            check_roundtrip(TraceId::from_raw(42), "0000000000000000000000001A");
            let id = ULID::from_raw(u128::MAX);
            check_roundtrip(id, "7ZZZZZZZZZZZZZZZZZZZZZZZZZ");
            let decoded: Row<ULID> =
                serde_json::from_str(r#"{"id":"ZZZZZZZZZZZZZZZZZZZZZZZZZZ"}"#).unwrap();
            assert_eq!(decoded.id, id);
        }
    }

    #[test]
    fn rejects_invalid_layouts() {
        check_invalid::<EvenId>(r#"{"id":"000000000001B"}"#, "decode overflow:");
        #[cfg(feature = "snowflake")]
        {
            check_invalid::<SnowflakeTwitterId>(r#"{"id":"FZZZZZZZZZZZZ"}"#, "decode overflow:");
            check_invalid::<EventId>(r#"{"id":"7ZZZZZZ"}"#, "decode overflow:");
        }
        #[cfg(feature = "ulid")]
        check_invalid::<TraceId>(r#"{"id":"ZZZZZZZZZZZZZZZZZZZZZZZZZZ"}"#, "decode overflow:");
    }

    #[test]
    fn rejects_malformed_input() {
        for (json, expected) in [
            (r#"{"id":"bad"}"#, "invalid length:"),
            (r#"{"id":"00000000000@0"}"#, "invalid ascii byte"),
            (r#"{"id":42}"#, "a Crockford base32 encoded string"),
        ] {
            check_invalid::<EvenId>(json, expected);
        }
    }

    #[test]
    fn serialization_preserves_unchecked_bits() {
        assert_eq!(
            serde_json::to_string(&Row { id: EvenId(43) }).unwrap(),
            r#"{"id":"000000000001B"}"#
        );
    }
}
