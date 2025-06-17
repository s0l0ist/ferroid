mod snowflake;
mod ulid;

pub use snowflake::*;
pub use ulid::*;

use std::fmt;
use std::hash::Hash;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign};

// Common trait that captures the shared behavior
pub trait Id:
    Copy + Clone + fmt::Display + PartialOrd + Ord + PartialEq + Eq + Hash + fmt::Debug
{
    /// Scalar type for all bit fields (typically `u64` or `u128`)
    type Ty: Copy
        + Clone
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
        + Into<Self::Ty>
        + From<Self::Ty>;

    /// Converts this type into its raw type representation
    fn to_raw(&self) -> Self::Ty;

    /// Converts a raw type into this type
    fn from_raw(raw: Self::Ty) -> Self;
}
