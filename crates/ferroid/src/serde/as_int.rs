//! Serialize IDs as their native backing integer.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{Error, id::Id};

/// Serializes an ID as its native integer representation.
///
/// # Errors
///
/// Returns an error if the serializer fails.
pub fn serialize<ID, S>(id: &ID, serializer: S) -> Result<S::Ok, S::Error>
where
    ID: Id,
    ID::Ty: Serialize,
    S: Serializer,
{
    id.to_raw().serialize(serializer)
}

/// Deserializes an ID from its native integer using its checked [`TryFrom`] conversion.
///
/// # Errors
///
/// Returns an error if integer deserialization fails or the ID is invalid.
pub fn deserialize<'de, ID, D>(deserializer: D) -> Result<ID, D::Error>
where
    ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
    ID::Ty: Deserialize<'de>,
    D: Deserializer<'de>,
{
    ID::try_from(ID::Ty::deserialize(deserializer)?).map_err(serde::de::Error::custom)
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::ToString};
    use serde::{Deserialize, Serialize};

    use crate::{Error, id::Id, serde::tests::EvenId};
    #[cfg(feature = "snowflake")]
    use crate::{id::SnowflakeTwitterId, serde::tests::EventId};
    #[cfg(feature = "ulid")]
    use crate::{id::ULID, serde::tests::TraceId};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(bound(
        serialize = "ID::Ty: Serialize",
        deserialize = "ID::Ty: Deserialize<'de>, ID: TryFrom<<ID as Id>::Ty, Error = Error<ID>>"
    ))]
    struct Row<ID: Id> {
        #[serde(with = "super")]
        id: ID,
    }

    fn check_roundtrip<ID>(id: ID, expected: &str)
    where
        ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
        ID::Ty: Serialize + serde::de::DeserializeOwned,
    {
        let row = Row { id };
        let json = serde_json::to_string(&row).unwrap();
        assert_eq!(json, format!("{{\"id\":{expected}}}"));
        assert_eq!(serde_json::from_str::<Row<ID>>(&json).unwrap(), row);
    }

    fn check_invalid<ID>(json: &str)
    where
        ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
        ID::Ty: serde::de::DeserializeOwned,
    {
        let error = serde_json::from_str::<Row<ID>>(json).unwrap_err();
        assert!(error.to_string().contains("decode overflow:"), "{error}");
    }

    #[test]
    fn roundtrips_builtin_and_custom_ids() {
        check_roundtrip(EvenId(42), "42");
        #[cfg(feature = "snowflake")]
        {
            check_roundtrip(SnowflakeTwitterId::from_raw(42), "42");
            check_roundtrip(EventId::from_raw(42), "42");
        }
        #[cfg(feature = "ulid")]
        {
            check_roundtrip(TraceId::from_raw(42), "42");
            check_roundtrip(
                ULID::from_raw(u128::MAX),
                "340282366920938463463374607431768211455",
            );
        }
    }

    #[test]
    fn rejects_invalid_layouts() {
        check_invalid::<EvenId>(r#"{"id":43}"#);
        #[cfg(feature = "snowflake")]
        {
            check_invalid::<SnowflakeTwitterId>(r#"{"id":18446744073709551615}"#);
            check_invalid::<EventId>(r#"{"id":4294967295}"#);
        }
        #[cfg(feature = "ulid")]
        check_invalid::<TraceId>(r#"{"id":340282366920938463463374607431768211455}"#);
    }

    #[test]
    fn serialization_preserves_unchecked_bits() {
        assert_eq!(
            serde_json::to_string(&Row { id: EvenId(43) }).unwrap(),
            r#"{"id":43}"#
        );
    }
}
