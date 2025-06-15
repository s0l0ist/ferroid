//! # Flexible ULID-style ID Generator
//!
//! This module provides a macro-based system for defining layout-safe,
//! monotonic ULID-style IDs using a customizable bit partition between
//! `timestamp` and `randomness`.
//!
//! Unlike Snowflake IDs, FUIDs do not include machine identifiers or sequences.
//! Instead, they prioritize compact encoding of wall-clock time and entropy.
//!
//! Example usage:
//!
//! ```
//! use ferroid::define_fluid_id;
//!
//! define_fluid_id!(
//!     MyFuid, u128,
//!     timestamp: 48,
//!     randomness: 80
//! );
//!
//! let id = MyFuid::from_components(1_725_000_000_000, 0xdeadbeef);
//! assert_eq!(id.timestamp(), 1_725_000_000_000);
//! ```

use core::{
    fmt,
    hash::Hash,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

/// Trait for layout-compatible ULID-style identifiers.
///
/// This trait abstracts a `timestamp` and `randomness` partition over a
/// fixed-size integer (e.g., `u128`) used for high-entropy time-sortable ID
/// generation.
///
/// Types implementing `Fluid` expose methods for construction, encoding, and
/// extracting field components from packed integers.
///
/// Unlike `Snowflake`, this trait does not assume a sequence or machine ID.
pub trait Fluid:
    Copy + Clone + fmt::Display + PartialOrd + Ord + PartialEq + Eq + Hash + fmt::Debug
{
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
        + fmt::Debug
        + fmt::Display;

    /// Returns the timestamp portion of the ID.
    fn timestamp(&self) -> Self::Ty;

    /// Returns the randomness portion of the ID.
    fn randomness(&self) -> Self::Ty;

    /// Returns the maximum possible value for the timestamp field.
    fn max_timestamp() -> Self::Ty;

    /// Returns the maximum possible value for the randomness field.
    fn max_randomness() -> Self::Ty;

    /// Constructs a new Fluid from its components.
    fn from_components(timestamp: Self::Ty, randomness: Self::Ty) -> Self;

    /// Converts this type into its raw type representation
    fn to_raw(&self) -> Self::Ty;

    /// Converts a raw type into this type
    fn from_raw(raw: Self::Ty) -> Self;

    fn to_padded_string(&self) -> String;
}

/// Declares a `FluID`-compatible type with custom timestamp and randomness bit
/// layouts.
///
/// This macro defines a packed ID structure using a fixed-width integer (e.g.,
/// `u128`) and generates field masks and accessors to extract each component.
///
/// All bits must be fully accounted for; otherwise, a compile-time assertion
/// will fail.
///
/// ## Bit layout
///
/// The ID is packed from **MSB to LSB**:
///
/// ```text
///  Bit Index:  high bits               low bits
///              +---------------+----------------+
///  Field:      | timestamp (N) | randomness (M) |
///              +---------------+----------------+
///              |<--- MSB -- 128 bits -- LSB --->|
/// ```
///
/// ## Example
///
/// ```
/// define_flu_id!(
///     MyUlid, u128,
///     timestamp: 48,
///     randomness: 80
/// );
/// ```
///
/// This creates a type `MyUlid` with:
///
/// - 48 bits for the timestamp (stored in the upper bits)
/// - 80 bits of randomness (lower bits)
#[macro_export]
macro_rules! define_flu_id {
    (
        $(#[$meta:meta])*
        $name:ident, $int:ty,
        reserved: $reserved_bits:expr,
        timestamp: $timestamp_bits:expr,
        random: $random_bits:expr
    ) => {
        $(#[$meta])*
        #[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name {
            id: $int,
        }

        const _: () = {
            // Compile-time check: total bit width _must_ equal the backing
            // type. This is to avoid aliasing surprises.
            assert!(
                $reserved_bits + $timestamp_bits + $random_bits == <$int>::BITS,
                "Fluid layout overflows the underlying integer type"
            );
        };

        impl $name {
            pub const RESERVED_BITS: $int = $reserved_bits;
            pub const TIMESTAMP_BITS: $int = $timestamp_bits;
            pub const RANDOM_BITS: $int = $random_bits;

            pub const RANDOM_SHIFT: $int = 0;
            pub const TIMESTAMP_SHIFT: $int = Self::RANDOM_SHIFT + Self::RANDOM_BITS;
            pub const RESERVED_SHIFT: $int = Self::TIMESTAMP_SHIFT + Self::TIMESTAMP_BITS;

            pub const RESERVED_MASK: $int = ((1 << Self::RESERVED_BITS) - 1);
            pub const TIMESTAMP_MASK: $int = ((1 << Self::TIMESTAMP_BITS) - 1);
            pub const RANDOM_MASK: $int = ((1 << Self::RANDOM_BITS) - 1);

            pub const fn from(timestamp: $int, random: $int) -> Self {
                let t = (timestamp & Self::TIMESTAMP_MASK) << Self::TIMESTAMP_SHIFT;
                let r = (random & Self::RANDOM_MASK) << Self::RANDOM_SHIFT;
                Self { id: t | r }
            }


            /// Extracts the timestamp from the packed ID.
            pub const fn timestamp(&self) -> $int {
                (self.id >> Self::TIMESTAMP_SHIFT) & Self::TIMESTAMP_MASK
            }

            /// Extracts the timestamp from the packed ID.
            pub const fn randomness(&self) -> $int {
                (self.id >> Self::RANDOM_SHIFT) & Self::RANDOM_MASK
            }
            /// Returns the maximum representable timestamp value based on
            /// Self::TIMESTAMP_BITS.
            pub const fn max_timestamp() -> $int {
                (1 << Self::TIMESTAMP_BITS) - 1
            }
            /// Returns the maximum representable sequence value based on
            /// Self::RANDOM_BITS.
            pub const fn max_randomness() -> $int {
                (1 << Self::RANDOM_BITS) - 1
            }
        }

        impl $crate::Fluid for $name {
            type Ty = $int;

            fn timestamp(&self) -> Self::Ty {
                self.timestamp()
            }


            fn randomness(&self) -> Self::Ty {
                self.randomness()
            }

            fn max_timestamp() -> Self::Ty {
                (1 << $timestamp_bits) - 1
            }

            fn max_randomness() -> Self::Ty {
                (1 << $random_bits) - 1
            }

            fn from_components(timestamp: $int, randomness: $int) -> Self {
                debug_assert!(timestamp <= Self::TIMESTAMP_MASK, "timestamp overflow");
                Self::from(timestamp, randomness)
            }


            fn to_raw(&self) -> Self::Ty {
                self.id
            }

            fn from_raw(raw: Self::Ty) -> Self {
                 Self { id: raw }
            }

            fn to_padded_string(&self) -> String {
                let max = Self::Ty::MAX;
                let mut n = max;
                let mut digits = 1;
                while n >= 10 {
                    n /= 10;
                    digits += 1;
                }
                format!("{:0width$}", self.to_raw(), width = digits)
            }
        }

        impl core::fmt::Display for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                write!(f, "{}", self.id)
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let full = core::any::type_name::<Self>();
                let name = full.rsplit("::").next().unwrap_or(full);
                let mut dbg = f.debug_struct(name);
                dbg.field("id", &format_args!("{:} (0x{:x})", self.to_raw(), self.to_raw()));

                use $crate::Fluid;
                dbg.field("padded", &self.to_padded_string());

                #[cfg(feature = "base32")]
                {
                    use $crate::FluidBase32Ext;
                    dbg.field("base32", &self.encode());
                }

                dbg.field("timestamp", &format_args!("{:} (0x{:x})", self.timestamp(), self.timestamp()));
                dbg.field("randomness", &format_args!("{:} (0x{:x})", self.randomness(), self.randomness()));

                dbg.finish()
            }
        }
    };
}

define_flu_id!(
    /// A 128-bit FUID using the ULID layout
    ///
    /// - 48 bits timestamp (ms since [`CUSTOM_EPOCH`])
    /// - 80 bits randomness
    ///
    /// ```text
    ///  Bit Index:  127            80 79             0
    ///              +----------------+---------------+
    ///  Field:      | timestamp (48) | sequence (80) |
    ///              +----------------+---------------+
    ///              |<--- MSB -- 128 bits -- LSB --->|
    /// ```
    /// [`CUSTOM_EPOCH`]: crate::CUSTOM_EPOCH
    Ulid, u128,
    reserved: 0,
    timestamp: 48,
    random: 80
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snowflake_ulid_id_fields_and_bounds() {
        let ts = Ulid::max_timestamp();
        let rand = Ulid::max_randomness();

        let id = Ulid::from(ts, rand);
        println!("ID: {:#?}", id);
        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.randomness(), rand);
        assert_eq!(Ulid::from_components(ts, rand), id);
    }

    #[test]
    #[should_panic(expected = "timestamp overflow")]
    fn ulid_timestamp_overflow_panics() {
        let ts = Ulid::max_timestamp() + 1;
        Ulid::from_components(ts, 0);
    }

    #[test]
    #[should_panic(expected = "randomness overflow")]
    fn ulid_randomness_overflow_panics() {
        let rand = Ulid::max_randomness() + 1;
        Ulid::from_components(0, rand);
    }

    #[test]
    fn ulid_low_bit_fields() {
        let id = Ulid::from_components(0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.randomness(), 0);

        let id = Ulid::from_components(1, 1);
        assert_eq!(id.timestamp(), 1);
        assert_eq!(id.randomness(), 1);
    }
}
