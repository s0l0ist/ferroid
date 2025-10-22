use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod as_native_snow {
    use super::{Deserialize, Deserializer, Serialize, Serializer};
    use crate::{SerdeError, SnowflakeId};

    /// Serialize a snowflake ID as its native integer representation.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying serializer fails.
    pub fn serialize<ID, S>(id: &ID, s: S) -> Result<S::Ok, S::Error>
    where
        ID: SnowflakeId,
        ID::Ty: Serialize,
        S: Serializer,
    {
        id.to_raw().serialize(s)
    }

    /// Deserialize a snowflake ID from its native integer representation.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The underlying deserializer fails
    /// - The deserialized value is not a valid snowflake ID (e.g., exceeds the
    ///   valid range)
    pub fn deserialize<'de, ID, D>(d: D) -> Result<ID, D::Error>
    where
        ID: SnowflakeId,
        ID::Ty: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let n = <ID::Ty>::deserialize(d)?;
        let id = ID::from_raw(n);
        if !id.is_valid() {
            return Err(serde::de::Error::custom(SerdeError::DecodeOverflow { id }));
        }
        Ok(id)
    }
}

#[cfg_attr(
    docsrs,
    doc(cfg(all(feature = "serde", feature = "snowflake", feature = "base32")))
)]
#[cfg(feature = "base32")]
pub mod as_base32_snow {
    use super::{Deserializer, Serializer};
    use crate::{Base32SnowExt, BeBytes, SerdeError};

    /// Serialize a snowflake ID as a Crockford base32 encoded string.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying serializer fails.
    pub fn serialize<ID, S>(id: &ID, s: S) -> Result<S::Ok, S::Error>
    where
        ID: Base32SnowExt,
        ID::Ty: BeBytes,
        S: Serializer,
    {
        s.serialize_str(id.encode().as_str())
    }

    /// Deserialize a snowflake ID from a Crockford base32 encoded string.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The underlying deserializer fails
    /// - The string is not valid Crockford base32 (invalid length or ascii)
    /// - The decoded value is not a valid snowflake ID (e.g., exceeds the valid
    ///   range)
    pub fn deserialize<'de, ID, D>(d: D) -> Result<ID, D::Error>
    where
        ID: Base32SnowExt,
        ID::Ty: BeBytes,
        D: Deserializer<'de>,
    {
        struct Base32Visitor<ID>(core::marker::PhantomData<ID>);

        impl<ID> serde::de::Visitor<'_> for Base32Visitor<ID>
        where
            ID: Base32SnowExt,
            ID::Ty: BeBytes,
        {
            type Value = ID;

            fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
                formatter.write_str("a Crockford base32 encoded string")
            }

            #[inline]
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                ID::decode(v).map_err(|e| serde::de::Error::custom(SerdeError::Base32Error(e)))
            }
        }

        d.deserialize_str(Base32Visitor(core::marker::PhantomData))
    }
}

#[cfg(all(test, feature = "alloc", feature = "snowflake"))]
mod tests {
    use super::*;
    use crate::{SerdeError, SnowflakeTwitterId};
    use alloc::string::ToString;
    use core::u64;
    use serde_json::json;

    #[test]
    fn native_snow_roundtrip() {
        #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
        struct Row {
            #[serde(with = "as_native_snow")]
            event_id: SnowflakeTwitterId,
        }
        let row = Row {
            event_id: SnowflakeTwitterId::from_raw(42),
        };

        let json = serde_json::to_string(&row).expect("serialize");
        assert_eq!(json, json!(r#"{"event_id":42}"#));
        let back: Row = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, row);
    }

    #[test]
    fn native_snow_roundtrip_decode_overflow() {
        #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
        struct Row {
            #[serde(with = "as_native_snow")]
            event_id: SnowflakeTwitterId,
        }
        let json = json!({"event_id": u64::MAX});
        let err = serde_json::from_value::<Row>(json).expect_err("should fail");
        assert_eq!(
            err.to_string(),
            SerdeError::DecodeOverflow {
                id: SnowflakeTwitterId::from_raw(u64::MAX)
            }
            .to_string()
        );
    }

    #[test]
    #[cfg(feature = "base32")]
    fn base32_snow_roundtrip() {
        #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
        struct Row {
            #[serde(with = "as_base32_snow")]
            event_id: SnowflakeTwitterId,
        }
        let row = Row {
            event_id: SnowflakeTwitterId::from_raw(42),
        };

        let json = serde_json::to_string(&row).expect("serialize");
        assert_eq!(json, json!(r#"{"event_id":"000000000001A"}"#));
        let back: Row = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, row);
    }

    #[test]
    #[cfg(feature = "base32")]
    fn base32_snow_roundtrip_decode_overflow() {
        use crate::Base32Error;

        #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
        struct Row {
            #[serde(with = "as_base32_snow")]
            event_id: SnowflakeTwitterId,
        }
        let json = json!({"event_id":"FZZZZZZZZZZZZ"});
        let err = serde_json::from_value::<Row>(json).expect_err("should fail");
        assert_eq!(
            err.to_string(),
            SerdeError::Base32Error(Base32Error::DecodeOverflow {
                id: SnowflakeTwitterId::from_raw(u64::MAX)
            })
            .to_string()
        );
    }
}
