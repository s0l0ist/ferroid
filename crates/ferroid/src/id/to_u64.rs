/// Converts an unsigned numeric value to `u64`, saturating at [`u64::MAX`].
///
/// Backoff conversion uses this to cap suggested waits without narrowing the
/// timestamp values stored in IDs.
pub trait ToU64 {
    fn to_u64(self) -> u64;
}

impl ToU64 for u8 {
    fn to_u64(self) -> u64 {
        u64::from(self)
    }
}

impl ToU64 for u16 {
    fn to_u64(self) -> u64 {
        u64::from(self)
    }
}

impl ToU64 for u32 {
    fn to_u64(self) -> u64 {
        u64::from(self)
    }
}

impl ToU64 for u64 {
    fn to_u64(self) -> u64 {
        self
    }
}

impl ToU64 for u128 {
    fn to_u64(self) -> u64 {
        self.try_into().unwrap_or(u64::MAX)
    }
}
