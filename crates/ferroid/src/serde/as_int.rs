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
