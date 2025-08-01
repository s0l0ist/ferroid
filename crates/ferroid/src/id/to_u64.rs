/// Trait for converting numeric-like values into a `u64`.
///
/// This is typically used to normalize custom duration types into milliseconds
/// for compatibility with APIs like [`core::time::Duration::from_millis`],
/// which are commonly required in async sleep contexts.
///
/// # Safety and Behavior
///
/// For types that may exceed the `u64` range (e.g., `u128`), values that cannot
/// be losslessly converted will saturate to `u64::MAX`. This avoids propagating
/// errors in time-sensitive code like ID generation. In such systems, a
/// fallback to `u64::MAX` is generally safe: it typically causes a retry
/// without compromising correctness, since most sane ID formats reserve no more
/// than 48 bits for timestamps - far below the 64-bit boundary.
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
