use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod as_native_ulid {
    use super::*;
    use crate::{Error, SerdeError, UlidId};

    pub fn serialize<ID, S>(id: &ID, s: S) -> Result<S::Ok, S::Error>
    where
        ID: UlidId,
        ID::Ty: Serialize,
        S: Serializer,
    {
        id.to_raw().serialize(s)
    }

    pub fn deserialize<'de, ID, D>(d: D) -> Result<ID, D::Error>
    where
        ID: UlidId,
        ID::Ty: Deserialize<'de>,
        D: Deserializer<'de>,
    {
        let n = <ID::Ty>::deserialize(d)?;
        let id = ID::from_raw(n);
        if !id.is_valid() {
            return Err(serde::de::Error::custom(Error::SerdeError(
                SerdeError::DecodeOverflow { id },
            )));
        }
        Ok(id)
    }
}

#[cfg(feature = "base32")]
pub mod as_base32_ulid {
    use super::*;
    use crate::{Base32UlidExt, BeBytes, SerdeError};

    pub fn serialize<ID, S>(id: &ID, s: S) -> Result<S::Ok, S::Error>
    where
        ID: Base32UlidExt,
        ID::Ty: BeBytes,
        S: Serializer,
    {
        s.serialize_str(id.encode().as_str())
    }

    pub fn deserialize<'de, ID, D>(d: D) -> Result<ID, D::Error>
    where
        ID: Base32UlidExt,
        ID::Ty: BeBytes,
        D: Deserializer<'de>,
    {
        struct Base32Visitor<ID>(core::marker::PhantomData<ID>);

        impl<'de, ID> serde::de::Visitor<'de> for Base32Visitor<ID>
        where
            ID: Base32UlidExt,
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
                ID::decode(v).map_err(|e| {
                    let err = SerdeError::Base32Error(e);
                    serde::de::Error::custom(err)
                })
            }
        }

        d.deserialize_str(Base32Visitor(core::marker::PhantomData))
    }
}

#[cfg(all(test, feature = "ulid"))]
mod tests {
    use super::*;
    use crate::ULID;
    use core::u64;
    use serde_json::json;

    #[test]
    fn native_ulid_roundtrip() {
        #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
        struct Row {
            #[serde(with = "as_native_ulid")]
            event_id: ULID,
        }
        let row = Row {
            event_id: ULID::from_raw(42),
        };

        let json = serde_json::to_string(&row).expect("serialize");
        assert_eq!(json, json!(r#"{"event_id":42}"#));
        let back: Row = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, row);
    }

    #[test]
    #[cfg(feature = "base32")]
    fn base32_ulid_roundtrip() {
        #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
        struct Row {
            #[serde(with = "as_base32_ulid")]
            event_id: ULID,
        }
        let row = Row {
            event_id: ULID::from_raw(42),
        };

        let json = serde_json::to_string(&row).expect("serialize");
        assert_eq!(json, json!(r#"{"event_id":"0000000000000000000000001A"}"#));
        let back: Row = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, row);
    }
}
