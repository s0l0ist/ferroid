use crate::{Error, id::Id};

// This ID deliberately implements neither SnowflakeId nor UlidId.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) struct EvenId(pub(super) u64);

impl Id for EvenId {
    const ZERO: Self::Ty = 0;
    const ONE: Self::Ty = 1;
    type Ty = u64;

    fn to_raw(self) -> Self::Ty {
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

#[cfg(feature = "snowflake")]
crate::define_snowflake_id!(EventId, u32,
    reserved: 1, timestamp: 18, machine_id: 5, sequence: 8);

#[cfg(feature = "ulid")]
crate::define_ulid!(TraceId, u128, reserved: 1, timestamp: 47, random: 80);
