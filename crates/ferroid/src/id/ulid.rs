use crate::Id;
use core::{fmt, hash::Hash};

/// Trait for layout-compatible ULID-style identifiers.
///
/// This trait abstracts a `timestamp`, `random` , and `sequence` partitions
/// over a fixed-size integer (e.g., `u128`) used for high-entropy time-sortable
/// ID generation.
///
/// Types implementing `Ulid` expose methods for construction, encoding, and
/// extracting field components from packed integers.
pub trait Ulid:
    Id + Copy + Clone + fmt::Display + PartialOrd + Ord + PartialEq + Eq + Hash + fmt::Debug
{
    /// Returns the timestamp portion of the ID.
    fn timestamp(&self) -> Self::Ty;

    /// Returns the random portion of the ID.
    fn random(&self) -> Self::Ty;

    /// Returns the maximum possible value for the timestamp field.
    fn max_timestamp() -> Self::Ty;

    /// Returns the maximum possible value for the random field.
    fn max_random() -> Self::Ty;

    /// Constructs a new ULID from its components.
    fn from_components(timestamp: Self::Ty, random: Self::Ty) -> Self;

    /// Returns true if the current sequence value can be incremented.
    fn has_random_room(&self) -> bool {
        self.random() < Self::max_random()
    }

    /// Returns the next sequence value.
    fn next_random(&self) -> Self::Ty {
        self.random() + Self::ONE
    }

    /// Returns a new ID with the random portion incremented.
    fn increment_random(&self) -> Self {
        Self::from_components(self.timestamp(), self.next_random())
    }

    /// Returns a new ID for a newer timestamp with sequence reset to zero.
    fn rollover_to_timestamp(&self, ts: Self::Ty, rand: Self::Ty) -> Self {
        Self::from_components(ts, rand)
    }

    /// Returns `true` if the ID's internal structure is valid, such as reserved
    /// bits being unset or fields within expected ranges.
    fn is_valid(&self) -> bool;

    /// Returns a normalized version of the ID with any invalid or reserved bits
    /// cleared. This guarantees a valid, canonical representation.
    fn into_valid(self) -> Self;

    fn to_padded_string(&self) -> String;
}

/// # Field Ordering Semantics
///
/// The `define_ulid!` macro defines a bit layout for a custom Ulid using four
/// required components: `reserved`, `timestamp`, and `random`.
///
/// These components are always laid out from **most significant bit (MSB)** to
/// **least significant bit (LSB)** - in that exact order.
///
/// - The first field (`reserved`) occupies the highest bits.
/// - The last field (`random`) occupies the lowest bits.
/// - The total number of bits **must exactly equal** the size of the backing
///   integer type (`u64`, `u128`, etc.). If it doesn't, the macro will trigger
///   a compile-time assertion failure.
///
/// ```text
/// define_ulid!(
///     <TypeName>, <IntegerType>,
///     reserved: <bits>,
///     timestamp: <bits>,
///     random: <bits>
/// );
///```
///
/// ## Example: A non-monotonic ULID layout
/// ```rust
/// use ferroid::define_ulid;
///
/// define_ulid!(
///     MyCustomId, u128,
///     reserved: 0,
///     timestamp: 48,
///     random: 80
/// );
/// ```
///
/// Which expands to the following bit layout:
///
/// ```text
///  Bit Index:  127            80 79           0
///              +----------------+-------------+
///  Field:      | timestamp (48) | random (80) |
///              +----------------+-------------+
///              |<-- MSB -- 128 bits -- LSB -->|
/// ```
#[macro_export]
macro_rules! define_ulid {
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
                "Ulid layout overflows the underlying integer type"
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
            /// Extracts the random number from the packed ID.
            pub const fn random(&self) -> $int {
                (self.id >> Self::RANDOM_SHIFT) & Self::RANDOM_MASK
            }
            /// Returns the maximum representable timestamp value based on
            /// Self::TIMESTAMP_BITS.
            pub const fn max_timestamp() -> $int {
                (1 << Self::TIMESTAMP_BITS) - 1
            }
            /// Returns the maximum representable randome value based on
            /// Self::RANDOM_BITS.
            pub const fn max_random() -> $int {
                (1 << Self::RANDOM_BITS) - 1
            }

            /// Converts this type into its raw type representation
            pub const fn to_raw(&self) -> $int {
                self.id
            }

            /// Converts a raw type into this type
            pub const fn from_raw(raw: $int) -> Self {
                Self { id: raw }
            }
        }

        impl $crate::Id for $name {
            type Ty = $int;
            const ZERO: $int = 0;
            const ONE: $int = 1;

            /// Converts this type into its raw type representation
            fn to_raw(&self) -> Self::Ty {
                self.id
            }

            /// Converts a raw type into this type
            fn from_raw(raw: Self::Ty) -> Self {
                Self { id: raw }
            }
        }

        impl $crate::Ulid for $name {
            fn timestamp(&self) -> Self::Ty {
                self.timestamp()
            }

            fn random(&self) -> Self::Ty {
                self.random()
            }

            fn max_timestamp() -> Self::Ty {
                (1 << $timestamp_bits) - 1
            }

            fn max_random() -> Self::Ty {
                (1 << $random_bits) - 1
            }

            fn from_components(timestamp: $int, random: $int) -> Self {
                // Random bits can frequencly overflow, but this is okay since
                // they're masked. We don't need a debug assertion here because
                // this is expected behavior. However, the timestamp and part
                // should never overflow.
                debug_assert!(timestamp <= Self::TIMESTAMP_MASK, "timestamp overflow");
                Self::from(timestamp, random)
            }

            fn is_valid(&self) -> bool {
                *self == Self::from_components(self.timestamp(), self.random())
            }

            fn into_valid(self) -> Self {
                Self::from_components(self.timestamp(), self.random())
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
                use $crate::Ulid;

                let full = core::any::type_name::<Self>();
                let name = full.rsplit("::").next().unwrap_or(full);
                let mut dbg = f.debug_struct(name);
                dbg.field("id", &format_args!("{:} (0x{:x})", self.to_raw(), self.to_raw()));
                dbg.field("padded", &self.to_padded_string());
                dbg.field("timestamp", &format_args!("{:} (0x{:x})", self.timestamp(), self.timestamp()));
                dbg.field("random", &format_args!("{:} (0x{:x})", self.random(), self.random()));
                dbg.finish()
            }
        }
    };
}

define_ulid!(
    /// A 128-bit Ulid using the ULID layout
    ///
    /// - 0 bits reserved
    /// - 48 bits timestamp
    /// - 80 bits random
    ///
    /// ```text
    ///  Bit Index:  127            80 79           0
    ///              +----------------+-------------+
    ///  Field:      | timestamp (48) | random (80) |
    ///              +----------------+-------------+
    ///              |<-- MSB -- 128 bits -- LSB -->|
    /// ```
    ULID, u128,
    reserved: 0,
    timestamp: 48,
    random: 80
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ulid_id_fields_and_bounds() {
        let ts = ULID::max_timestamp();
        let rand = ULID::max_random();

        let id = ULID::from(ts, rand);
        println!("ID: {:#?}", id);
        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.random(), rand);
        assert_eq!(ULID::from_components(ts, rand), id);
    }

    #[test]
    fn ulid_low_bit_fields() {
        let id = ULID::from_components(0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.random(), 0);

        let id = ULID::from_components(1, 1);
        assert_eq!(id.timestamp(), 1);
        assert_eq!(id.random(), 1);
    }
}
