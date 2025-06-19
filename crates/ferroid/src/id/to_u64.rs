use crate::{Error, Result};

/// Trait for converting numeric-like values into a `u64`.
///
/// This is typically used to normalize custom duration types into milliseconds
/// for compatibility with APIs like [`core::time::Duration::from_millis`],
/// which are commonly required in async sleep contexts.
pub trait ToU64 {
    fn to_u64(self) -> Result<u64>;
}

impl ToU64 for u8 {
    fn to_u64(self) -> Result<u64> {
        Ok(self as u64)
    }
}

impl ToU64 for u16 {
    fn to_u64(self) -> Result<u64> {
        Ok(self as u64)
    }
}

impl ToU64 for u32 {
    fn to_u64(self) -> Result<u64> {
        Ok(self as u64)
    }
}

impl ToU64 for u64 {
    fn to_u64(self) -> Result<u64> {
        Ok(self)
    }
}

impl ToU64 for u128 {
    fn to_u64(self) -> Result<u64> {
        self.try_into().map_err(|_| Error::FailedToU64)
    }
}
