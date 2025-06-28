use crate::ToU64;
use std::fmt;
use std::hash::Hash;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

// Common trait that captures the shared behavior
pub trait Id:
    Copy + Clone + fmt::Display + PartialOrd + Ord + PartialEq + Eq + Hash + fmt::Debug
{
    /// Zero value (used for resetting the sequence)
    const ZERO: Self::Ty;

    /// One value (used for incrementing the sequence)
    const ONE: Self::Ty;

    /// Scalar type for all bit fields (typically `u64` or `u128`)
    type Ty: Copy
        + Clone
        + Default
        + Add<Output = Self::Ty>
        + AddAssign
        + Sub<Output = Self::Ty>
        + SubAssign
        + Mul<Output = Self::Ty>
        + MulAssign
        + Div<Output = Self::Ty>
        + DivAssign
        + Ord
        + PartialOrd
        + Eq
        + PartialEq
        + Hash
        + ToU64
        + fmt::Debug
        + fmt::Display
        + From<u8>
        + core::ops::Shl<usize, Output = Self::Ty>
        + core::ops::Shr<usize, Output = Self::Ty>
        + core::ops::ShlAssign
        + core::ops::ShrAssign
        + core::ops::BitOr<Output = Self::Ty>
        + core::ops::BitAnd<Output = Self::Ty>
        + core::ops::BitXor<Output = Self::Ty>
        + core::ops::BitAndAssign
        + core::ops::BitOrAssign
        + core::ops::BitXorAssign
        + Into<Self::Ty>
        + From<Self::Ty>;

    /// Converts this type into its raw type representation
    fn to_raw(&self) -> Self::Ty;

    /// Converts a raw type into this type
    fn from_raw(raw: Self::Ty) -> Self;
}
