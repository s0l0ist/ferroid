use crate::ToU64;
use core::fmt;
use core::hash::Hash;
use core::ops::{
    Add, AddAssign, BitAnd, BitAndAssign, BitOr, BitOrAssign, BitXor, BitXorAssign, Div, DivAssign,
    Mul, MulAssign, Not, Shl, ShlAssign, Shr, ShrAssign, Sub, SubAssign,
};

/// A trait for types that wrap a primitive scalar identifier.
///
/// This is used to abstract over the raw scalar type behind an ID (e.g., `u64`,
/// `u128`).
///
/// Types implementing `Id` must define a scalar type `Ty` and provide
/// conversion to/from this raw representation.
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
        + fmt::Debug
        + fmt::Display
        + Ord
        + PartialOrd
        + Eq
        + PartialEq
        + Hash
        // For clock millis and testing
        + ToU64
        // For base32 decode and testing
        + From<u8>
        // Arithmetic
        + Add<Output = Self::Ty>
        + AddAssign<Self::Ty>
        + Sub<Output = Self::Ty>
        + SubAssign<Self::Ty>
        + Mul<Output = Self::Ty>
        + MulAssign<Self::Ty>
        + Div<Output = Self::Ty>
        + DivAssign<Self::Ty>
        // Bitwise
        + BitOr<Output = Self::Ty>
        + BitOrAssign<Self::Ty>
        + BitAnd<Output = Self::Ty>
        + BitAndAssign<Self::Ty>
        + BitXor<Output = Self::Ty>
        + BitXorAssign<Self::Ty>
        + Not<Output = Self::Ty>
        // Shifting
        + Shl<u8, Output = Self::Ty>
        + Shr<u8, Output = Self::Ty>
        + Shl<u32, Output = Self::Ty>
        + Shr<u32, Output = Self::Ty>
        + Shl<u64, Output = Self::Ty>
        + Shr<u64, Output = Self::Ty>
        + Shl<u128, Output = Self::Ty>
        + Shr<u128, Output = Self::Ty>
        + Shl<usize, Output = Self::Ty>
        + Shr<usize, Output = Self::Ty>
        + ShlAssign<Self::Ty>
        + ShrAssign<Self::Ty>;

    /// Converts this type into its raw type representation
    fn to_raw(&self) -> Self::Ty;

    /// Converts a raw type into this type
    fn from_raw(raw: Self::Ty) -> Self;
}
