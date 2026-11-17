use alloc::{format, string::ToString};

use serde::{Deserialize, Serialize};

#[cfg(feature = "base32")]
use super::as_base32;
use super::as_int;
#[cfg(feature = "base32")]
use crate::base32::Base32Ext;
#[cfg(feature = "base32")]
use crate::id::BeBytes;
use crate::{Error, id::Id};

#[cfg(feature = "snowflake")]
mod snowflake {
    use alloc::string::ToString;

    use serde_json::json;

    use super::*;
    use crate::id::SnowflakeTwitterId;

    #[test]
    fn native_snow_roundtrip() {
        #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
        struct Row {
            #[serde(with = "as_int")]
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
            #[serde(with = "as_int")]
            event_id: SnowflakeTwitterId,
        }
        let json = json!({"event_id": u64::MAX});
        let err = serde_json::from_value::<Row>(json).expect_err("should fail");
        assert_eq!(
            err.to_string(),
            Error::DecodeOverflow {
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
            #[serde(with = "as_base32")]
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
        #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
        struct Row {
            #[serde(with = "as_base32")]
            event_id: SnowflakeTwitterId,
        }
        let json = json!({"event_id":"FZZZZZZZZZZZZ"});
        let err = serde_json::from_value::<Row>(json).expect_err("should fail");
        assert_eq!(
            err.to_string(),
            Error::DecodeOverflow {
                id: SnowflakeTwitterId::from_raw(u64::MAX)
            }
            .to_string()
        );
    }
}

#[cfg(feature = "ulid")]
mod ulid {
    use serde_json::json;

    use super::*;
    use crate::id::ULID;

    #[test]
    fn native_ulid_roundtrip() {
        #[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
        struct Row {
            #[serde(with = "as_int")]
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
            #[serde(with = "as_base32")]
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

// This ID deliberately implements neither SnowflakeId nor UlidId.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct EvenId(u64);

impl Id for EvenId {
    const ZERO: Self::Ty = 0;
    const ONE: Self::Ty = 1;
    type Ty = u64;

    fn to_raw(&self) -> Self::Ty {
        self.0
    }

    fn from_raw(raw: Self::Ty) -> Self {
        Self(raw)
    }

    fn is_valid(&self) -> bool {
        self.0 & 1 == 0
    }

    fn into_valid(self) -> Self {
        Self(self.0 & !1)
    }
}

impl TryFrom<u64> for EvenId {
    type Error = Error<Self>;

    fn try_from(raw: u64) -> Result<Self, Self::Error> {
        let id = Self::from_raw(raw);
        if id.is_valid() {
            Ok(id)
        } else {
            Err(Error::DecodeOverflow { id })
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "ID::Ty: Serialize",
    deserialize = "ID::Ty: Deserialize<'de>, ID: TryFrom<<ID as Id>::Ty, Error = Error<ID>>"
))]
struct Integer<ID: Id> {
    #[serde(with = "as_int")]
    id: ID,
}

#[cfg(feature = "base32")]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "",
    deserialize = "ID: TryFrom<<ID as Id>::Ty, Error = Error<ID>>"
))]
struct Base32<ID: Id>
where
    ID::Ty: BeBytes,
{
    #[serde(with = "as_base32")]
    id: ID,
}

fn assert_integer_roundtrip<ID>(id: ID, expected: &str)
where
    ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
    ID::Ty: Serialize + serde::de::DeserializeOwned,
{
    let row = Integer { id };
    let encoded = serde_json::to_string(&row).unwrap();
    assert_eq!(encoded, format!("{{\"id\":{expected}}}"));
    let decoded: Integer<ID> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, row);
}

#[cfg(feature = "base32")]
fn assert_base32_roundtrip<ID>(id: ID, expected: &str)
where
    ID: Id + TryFrom<<ID as Id>::Ty, Error = Error<ID>>,
    ID::Ty: BeBytes,
{
    assert_eq!(id.encode().as_str(), expected);
    let row = Base32 { id };
    let encoded = serde_json::to_string(&row).unwrap();
    assert_eq!(encoded, format!("{{\"id\":\"{expected}\"}}"));
    let decoded: Base32<ID> = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, row);
}

#[test]
fn integer_adapter_supports_ids_without_a_family() {
    assert_integer_roundtrip(EvenId(42), "42");
    let invalid = Integer { id: EvenId(43) };
    let encoded = serde_json::to_string(&invalid).unwrap();
    assert_eq!(encoded, r#"{"id":43}"#);
    let error = serde_json::from_str::<Integer<EvenId>>(&encoded).unwrap_err();
    assert!(error.to_string().contains("decode overflow:"));
}

#[cfg(feature = "base32")]
#[test]
fn base32_adapter_supports_ids_without_a_family() {
    assert_base32_roundtrip(EvenId(42), "000000000001A");
    let mut buffer = EvenId::base32_array();
    assert_eq!(
        EvenId(42).encode_to_buf(&mut buffer).as_str(),
        "000000000001A"
    );
    assert_eq!(EvenId::decode(buffer).unwrap(), EvenId(42));
    assert_eq!(EvenId::decode("000000000001B"), EvenId::try_from(43));
    let invalid = Base32 { id: EvenId(43) };
    let encoded = serde_json::to_string(&invalid).unwrap();
    assert_eq!(encoded, r#"{"id":"000000000001B"}"#);
    let error = serde_json::from_str::<Base32<EvenId>>(&encoded).unwrap_err();
    assert!(error.to_string().contains("decode overflow:"));
}

#[cfg(feature = "base32")]
#[test]
fn base32_adapter_rejects_invalid_lengths_and_characters() {
    for (input, expected_error) in [
        (r#"{"id":"bad"}"#, "invalid length:"),
        (r#"{"id":"00000000000@0"}"#, "invalid ascii byte"),
    ] {
        let error = serde_json::from_str::<Base32<EvenId>>(input).unwrap_err();
        assert!(error.to_string().contains(expected_error));
    }
}

#[cfg(feature = "snowflake")]
#[test]
#[allow(
    dead_code,
    clippy::wrong_self_convention,
    reason = "Layout fixtures generate a complete ID API; these tests exercise Serde."
)]
fn adapters_support_custom_snowflake_widths() {
    crate::define_snowflake_id!(EventId, u32,
        reserved: 1, timestamp: 18, machine_id: 5, sequence: 8);
    assert_integer_roundtrip(EventId::from_raw(42), "42");
    assert!(serde_json::from_str::<Integer<EventId>>(r#"{"id":4294967295}"#).is_err());
    #[cfg(feature = "base32")]
    {
        assert_base32_roundtrip(EventId::from_raw(42), "000001A");
        assert!(serde_json::from_str::<Base32<EventId>>(r#"{"id":"7ZZZZZZ"}"#).is_err());
    }
}

#[cfg(feature = "ulid")]
#[test]
#[allow(
    dead_code,
    clippy::wrong_self_convention,
    reason = "Layout fixtures generate a complete ID API; these tests exercise Serde."
)]
fn adapters_support_custom_ulid_layouts() {
    crate::define_ulid!(TraceId, u128, reserved: 1, timestamp: 47, random: 80);
    const INVALID: TraceId = TraceId::from_raw(u128::MAX);
    const NORMALIZED: TraceId = INVALID.into_valid();
    const { assert!(NORMALIZED.is_valid()) };
    assert!(!Id::is_valid(&INVALID));
    assert_eq!(Id::into_valid(INVALID), NORMALIZED);
    assert_eq!(NORMALIZED.to_raw(), u128::MAX >> 1);
    assert_integer_roundtrip(TraceId::from_raw(42), "42");
    assert!(
        serde_json::from_str::<Integer<TraceId>>(
            r#"{"id":340282366920938463463374607431768211455}"#
        )
        .is_err()
    );
    #[cfg(feature = "base32")]
    {
        assert_base32_roundtrip(TraceId::from_raw(42), "0000000000000000000000001A");
        assert!(
            serde_json::from_str::<Base32<TraceId>>(r#"{"id":"ZZZZZZZZZZZZZZZZZZZZZZZZZZ"}"#)
                .is_err()
        );
    }
}

#[cfg(feature = "ulid")]
#[test]
fn adapters_preserve_full_width_ulids() {
    let id = crate::id::ULID::from_raw(u128::MAX);
    assert_integer_roundtrip(id, "340282366920938463463374607431768211455");
    #[cfg(feature = "base32")]
    {
        assert_base32_roundtrip(id, "7ZZZZZZZZZZZZZZZZZZZZZZZZZ");
        let aliased: Base32<crate::id::ULID> =
            serde_json::from_str(r#"{"id":"ZZZZZZZZZZZZZZZZZZZZZZZZZZ"}"#).unwrap();
        assert_eq!(aliased.id, id);
    }
}
