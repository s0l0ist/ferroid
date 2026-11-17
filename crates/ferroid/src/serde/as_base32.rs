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
