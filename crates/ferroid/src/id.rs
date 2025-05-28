use crate::{Error, Result};
use core::fmt;
use std::{
    hash::Hash,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

/// Trait for converting numeric-like values into a `u64`.
///
/// This is typically used to normalize custom duration types into milliseconds
/// for compatibility with APIs like [`std::time::Duration::from_millis`], which
/// are commonly required in async sleep contexts such as
/// [`tokio::time::sleep`].
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

/// A trait representing a layout-compatible Snowflake ID generator.
///
/// This trait abstracts the core behavior of a Snowflake-style ID with separate
/// bit fields for timestamp, machine ID, and sequence.
///
/// Types implementing this trait can define custom bit layouts and time units.
///
/// # Example
///
/// ```
/// use ferroid::{Snowflake, SnowflakeTwitterId};
///
/// let id = SnowflakeTwitterId::from(1000, 2, 1);
/// assert_eq!(id.timestamp(), 1000);
/// assert_eq!(id.machine_id(), 2);
/// assert_eq!(id.sequence(), 1);
/// ```
pub trait Snowflake:
    Copy + Clone + fmt::Display + PartialOrd + Ord + PartialEq + Eq + Hash
{
    /// Scalar type for all bit fields (typically `u64`)
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
        + Into<Self::Ty>
        + From<Self::Ty>
        + Ord
        + PartialOrd
        + Eq
        + PartialEq
        + Hash
        + ToU64
        + fmt::Debug
        + fmt::Display;

    /// Zero value (used for resetting the sequence)
    const ZERO: Self::Ty;

    /// One value (used for incrementing the sequence)
    const ONE: Self::Ty;

    /// Returns the timestamp portion of the ID.
    fn timestamp(&self) -> Self::Ty;

    /// Returns the maximum possible value for the timestamp field.
    fn max_timestamp() -> Self::Ty;

    /// Returns the machine ID portion of the ID.
    fn machine_id(&self) -> Self::Ty;

    /// Returns the maximum possible value for the machine_id field.
    fn max_machine_id() -> Self::Ty;

    /// Returns the sequence portion of the ID.
    fn sequence(&self) -> Self::Ty;

    /// Returns the maximum possible value for the sequence field.
    fn max_sequence() -> Self::Ty;

    /// Constructs a new Snowflake ID from its components.
    fn from_components(timestamp: Self::Ty, machine_id: Self::Ty, sequence: Self::Ty) -> Self;

    /// Converts this type into its raw type representation
    fn to_raw(&self) -> Self::Ty;

    /// Converts a raw type into this type
    fn from_raw(raw: Self::Ty) -> Self;

    /// Returns true if the current sequence value can be incremented.
    fn has_sequence_room(&self) -> bool {
        self.sequence() < Self::max_sequence()
    }

    /// Returns the next sequence value.
    fn next_sequence(&self) -> Self::Ty {
        self.sequence() + Self::ONE
    }

    /// Returns a new ID with the sequence incremented.
    fn increment_sequence(&self) -> Self {
        Self::from_components(self.timestamp(), self.machine_id(), self.next_sequence())
    }

    /// Returns a new ID for a newer timestamp with sequence reset to zero.
    fn rollover_to_timestamp(&self, ts: Self::Ty) -> Self {
        Self::from_components(ts, self.machine_id(), Self::ZERO)
    }

    fn to_padded_string(&self) -> String;
}

/// # Field Ordering Semantics
///
/// The `define_snowflake_id!` macro defines a bit layout for a custom Snowflake
/// ID using four required components: `reserved`, `timestamp`, `machine_id`,
/// and `sequence`.
///
/// These components are always laid out from **most significant bit (MSB)** to
/// **least significant bit (LSB)** - in that exact order.
///
/// - The first field (`reserved`) occupies the highest bits.
/// - The last field (`sequence`) occupies the lowest bits.
/// - The total number of bits **must exactly equal** the size of the backing
///   integer type (`u64`, `u128`, etc.). If it doesn't, the macro will trigger
///   a compile-time assertion failure.
///
/// ```text
/// define_snowflake_id!(
///     <TypeName>, <IntegerType>,
///     reserved: <bits>,
///     timestamp: <bits>,
///     machine_id: <bits>,
///     sequence: <bits>
/// );
///```
///
/// ## Example: A Twitter-like layout
/// ```rust
/// use ferroid::define_snowflake_id;
///
/// define_snowflake_id!(
///     MyCustomId, u64,
///     reserved: 1,
///     timestamp: 41,
///     machine_id: 10,
///     sequence: 12
/// );
/// ```
///
/// Which expands to the following bit layout:
///
/// ```text
///  Bit Index:  63           63 62            22 21             12 11             0
///              +--------------+----------------+-----------------+---------------+
///  Field:      | reserved (1) | timestamp (41) | machine ID (10) | sequence (12) |
///              +--------------+----------------+-----------------+---------------+
///              |<----------- MSB ---------- 64 bits ----------- LSB ------------>|
/// ```
#[macro_export]
macro_rules! define_snowflake_id {
    (
        $(#[$meta:meta])*
        $name:ident, $int:ty,
        reserved: $reserved_bits:expr,
        timestamp: $timestamp_bits:expr,
        machine_id: $machine_bits:expr,
        sequence: $sequence_bits:expr
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
                $reserved_bits + $timestamp_bits + $machine_bits + $sequence_bits == <$int>::BITS,
                "Snowflake layout overflows the underlying integer type"
            );
        };

        impl $name {
            pub const RESERVED_BITS: $int = $reserved_bits;
            pub const TIMESTAMP_BITS: $int = $timestamp_bits;
            pub const MACHINE_ID_BITS: $int = $machine_bits;
            pub const SEQUENCE_BITS: $int = $sequence_bits;

            pub const SEQUENCE_SHIFT: $int = 0;
            pub const MACHINE_ID_SHIFT: $int = Self::SEQUENCE_SHIFT + Self::SEQUENCE_BITS;
            pub const TIMESTAMP_SHIFT: $int = Self::MACHINE_ID_SHIFT + Self::MACHINE_ID_BITS;
            pub const RESERVED_SHIFT: $int = Self::TIMESTAMP_SHIFT + Self::TIMESTAMP_BITS;

            pub const RESERVED_MASK: $int = ((1 << Self::RESERVED_BITS) - 1);
            pub const TIMESTAMP_MASK: $int = ((1 << Self::TIMESTAMP_BITS) - 1);
            pub const MACHINE_ID_MASK: $int = ((1 << Self::MACHINE_ID_BITS) - 1);
            pub const SEQUENCE_MASK: $int = ((1 << Self::SEQUENCE_BITS) - 1);

            pub const fn from(timestamp: $int, machine_id: $int, sequence: $int) -> Self {
                let t = (timestamp & Self::TIMESTAMP_MASK) << Self::TIMESTAMP_SHIFT;
                let m = (machine_id & Self::MACHINE_ID_MASK) << Self::MACHINE_ID_SHIFT;
                let s = (sequence & Self::SEQUENCE_MASK) << Self::SEQUENCE_SHIFT;
                Self { id: t | m | s }
            }

            /// Extracts the timestamp from the packed ID.
            pub const fn timestamp(&self) -> $int {
                (self.id >> Self::TIMESTAMP_SHIFT) & Self::TIMESTAMP_MASK
            }
            /// Extracts the machine ID from the packed ID.
            pub const fn machine_id(&self) -> $int {
                (self.id >> Self::MACHINE_ID_SHIFT) & Self::MACHINE_ID_MASK
            }
            /// Extracts the sequence number from the packed ID.
            pub const fn sequence(&self) -> $int {
                (self.id >> Self::SEQUENCE_SHIFT) & Self::SEQUENCE_MASK
            }
        }

        impl $crate::Snowflake for $name {
            type Ty = $int;

            const ZERO: $int = 0;
            const ONE: $int = 1;

            fn timestamp(&self) -> Self::Ty {
                self.timestamp()
            }

            fn machine_id(&self) -> Self::Ty {
                self.machine_id()
            }

            fn sequence(&self) -> Self::Ty {
                self.sequence()
            }

            fn max_timestamp() -> Self::Ty {
                (1 << $timestamp_bits) - 1
            }

            fn max_machine_id() -> Self::Ty {
                (1 << $machine_bits) - 1
            }

            fn max_sequence() -> Self::Ty {
                (1 << $sequence_bits) - 1
            }

            fn from_components(timestamp: $int, machine_id: $int, sequence: $int) -> Self {
                debug_assert!(timestamp <= Self::TIMESTAMP_MASK, "timestamp overflow");
                debug_assert!(machine_id <= Self::MACHINE_ID_MASK, "machine_id overflow");
                debug_assert!(sequence <= Self::SEQUENCE_MASK, "sequence overflow");
                Self::from(timestamp, machine_id, sequence)
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
                    n = n / 10;
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

                use $crate::Snowflake;
                dbg.field("padded", &self.to_padded_string());

                #[cfg(feature = "base32")]
                {
                    use $crate::SnowflakeBase32Ext;
                    dbg.field("base32", &self.encode());
                }

                dbg.field("timestamp", &format_args!("{:} (0x{:x})", self.timestamp(), self.timestamp()));
                dbg.field("machine_id", &format_args!("{:} (0x{:x})", self.machine_id(), self.machine_id()));
                dbg.field("sequence", &format_args!("{:} (0x{:x})", self.sequence(), self.sequence()));

                dbg.finish()
            }
        }
    };
}

define_snowflake_id!(
    /// A 64-bit Snowflake ID using the Twitter layout
    ///
    /// - 1 bit reserved
    /// - 41 bits timestamp (ms since [`TWITTER_EPOCH`])
    /// - 10 bits machine ID (worker ID (5) and process ID (5))
    /// - 12 bits sequence
    ///
    /// ```text
    ///  Bit Index:  63           63 62            22 21             12 11             0
    ///              +--------------+----------------+-----------------+---------------+
    ///  Field:      | reserved (1) | timestamp (41) | machine ID (10) | sequence (12) |
    ///              +--------------+----------------+-----------------+---------------+
    ///              |<----------- MSB ---------- 64 bits ----------- LSB ------------>|
    /// ```
    /// [`TWITTER_EPOCH`]: crate::TWITTER_EPOCH
    SnowflakeTwitterId, u64,
    reserved: 1,
    timestamp: 41,
    machine_id: 10,
    sequence: 12
);

define_snowflake_id!(
    /// A 64-bit Snowflake ID using the Discord layout
    ///
    /// - 42 bits timestamp (ms since [`DISCORD_EPOCH`])
    /// - 10 bits machine ID (worker ID (5) and process ID (5))
    /// - 12 bits sequence
    ///
    /// ```text
    ///  Bit Index:  63             22 21             12 11             0
    ///              +----------------+-----------------+---------------+
    ///  Field:      | timestamp (42) | machine ID (10) | sequence (12) |
    ///              +----------------+-----------------+---------------+
    ///              |<----- MSB ---------- 64 bits --------- LSB ----->|
    /// ```
    /// [`DISCORD_EPOCH`]: crate::DISCORD_EPOCH
    SnowflakeDiscordId, u64,
    reserved: 0,
    timestamp: 42,
    machine_id: 10,
    sequence: 12
);

define_snowflake_id!(
    /// A 64-bit Snowflake ID using the Mastodon layout
    ///
    /// - 48 bits timestamp (ms since [`MASTODON_EPOCH`])
    /// - 16 bits sequence
    ///
    /// ```text
    ///  Bit Index:  63             16 15             0
    ///              +----------------+---------------+
    ///  Field:      | timestamp (48) | sequence (16) |
    ///              +----------------+---------------+
    ///              |<--- MSB --- 64 bits -- LSB --->|
    /// ```
    /// [`MASTODON_EPOCH`]: crate::MASTODON_EPOCH
    SnowflakeMastodonId, u64,
    reserved: 0,
    timestamp: 48,
    machine_id: 0,
    sequence: 16
);

define_snowflake_id!(
    /// A 64-bit Snowflake ID using the Instagram layout
    ///
    /// - 41 bits timestamp (ms since [`INSTAGRAM_EPOCH`])
    /// - 13 bits machine ID
    /// - 10 bits sequence
    ///
    /// ```text
    ///  Bit Index:  63             23 22             10 9              0
    ///              +----------------+-----------------+---------------+
    ///  Field:      | timestamp (41) | machine ID (13) | sequence (10) |
    ///              +----------------+-----------------+---------------+
    ///              |<----- MSB ---------- 64 bits --------- LSB ----->|
    /// ```
    /// [`INSTAGRAM_EPOCH`]: crate::INSTAGRAM_EPOCH
    SnowflakeInstagramId, u64,
    reserved: 0,
    timestamp: 41,
    machine_id: 13,
    sequence: 10
);

define_snowflake_id!(
    /// A 128-bit Snowflake ID using a hybrid layout.
    ///
    /// - 40 bits reserved
    /// - 48 bits timestamp (ms since [`CUSTOM_EPOCH`])
    /// - 20 bits machine ID
    /// - 20 bits sequence
    ///
    /// ```text
    ///  Bit Index:  127                88 87            40 39             20 19             0
    ///              +--------------------+----------------+-----------------+---------------+
    ///  Field:      | reserved (40 bits) | timestamp (48) | machine ID (20) | sequence (20) |
    ///              +--------------------+----------------+-----------------+---------------+
    ///              |<--- HI 64 bits --->|<------------------- LO 64 bits ----------------->|
    ///              |<- MSB ------ LSB ->|<----- MSB ---------- 64 bits --------- LSB ----->|
    /// ```
    /// [`CUSTOM_EPOCH`]: crate::CUSTOM_EPOCH
    SnowflakeLongId, u128,
    reserved: 40,
    timestamp: 48,
    machine_id: 20,
    sequence: 20
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snowflake_twitter_id_fields_and_bounds() {
        let ts = SnowflakeTwitterId::max_timestamp();
        let mid = SnowflakeTwitterId::max_machine_id();
        let seq = SnowflakeTwitterId::max_sequence();

        let id = SnowflakeTwitterId::from(ts, mid, seq);
        println!("ID: {:#?}", id);
        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), mid);
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeTwitterId::from_components(ts, mid, seq), id);
    }

    #[test]
    fn test_snowflake_discord_id_fields_and_bounds() {
        let ts = SnowflakeDiscordId::max_timestamp();
        let mid = SnowflakeDiscordId::max_machine_id();
        let seq = SnowflakeDiscordId::max_sequence();

        let id = SnowflakeDiscordId::from(ts, mid, seq);
        println!("ID: {:#?}", id);
        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), mid);
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeDiscordId::from_components(ts, mid, seq), id);
    }

    #[test]
    fn test_snowflake_mastodon_id_fields_and_bounds() {
        let ts = SnowflakeMastodonId::max_timestamp();
        let mid = SnowflakeMastodonId::max_machine_id();
        let seq = SnowflakeMastodonId::max_sequence();

        let id = SnowflakeMastodonId::from(ts, mid, seq);
        println!("ID: {:#?}", id);
        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), 0); // machine_id is always zero
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeMastodonId::from_components(ts, 0, seq), id);
    }

    #[test]
    fn test_snowflake_instagram_id_fields_and_bounds() {
        let ts = SnowflakeInstagramId::max_timestamp();
        let mid = SnowflakeInstagramId::max_machine_id();
        let seq = SnowflakeInstagramId::max_sequence();

        let id = SnowflakeInstagramId::from(ts, mid, seq);
        println!("ID: {:#?}", id);
        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), mid);
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeInstagramId::from_components(ts, mid, seq), id);
    }

    #[test]
    fn test_snowflake_long_id_fields_and_bounds() {
        let ts = SnowflakeLongId::max_timestamp();
        let mid = SnowflakeLongId::max_machine_id();
        let seq = SnowflakeLongId::max_sequence();

        let id = SnowflakeLongId::from(ts, mid, seq);
        println!("ID: {:#?}", id);
        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), mid);
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeLongId::from_components(ts, mid, seq), id);
    }

    #[test]
    #[should_panic(expected = "timestamp overflow")]
    fn twitter_timestamp_overflow_panics() {
        let ts = SnowflakeTwitterId::max_timestamp() + 1;
        SnowflakeTwitterId::from_components(ts, 0, 0);
    }

    #[test]
    #[should_panic(expected = "machine_id overflow")]
    fn twitter_machine_id_overflow_panics() {
        let mid = SnowflakeTwitterId::max_machine_id() + 1;
        SnowflakeTwitterId::from_components(0, mid, 0);
    }

    #[test]
    #[should_panic(expected = "sequence overflow")]
    fn twitter_sequence_overflow_panics() {
        let seq = SnowflakeTwitterId::max_sequence() + 1;
        SnowflakeTwitterId::from_components(0, 0, seq);
    }

    #[test]
    #[should_panic(expected = "timestamp overflow")]
    fn discord_timestamp_overflow_panics() {
        let ts = SnowflakeDiscordId::max_timestamp() + 1;
        SnowflakeDiscordId::from_components(ts, 0, 0);
    }

    #[test]
    #[should_panic(expected = "machine_id overflow")]
    fn discord_machine_id_overflow_panics() {
        let mid = SnowflakeDiscordId::max_machine_id() + 1;
        SnowflakeDiscordId::from_components(0, mid, 0);
    }

    #[test]
    #[should_panic(expected = "sequence overflow")]
    fn discord_sequence_overflow_panics() {
        let seq = SnowflakeDiscordId::max_sequence() + 1;
        SnowflakeDiscordId::from_components(0, 0, seq);
    }

    #[test]
    #[should_panic(expected = "timestamp overflow")]
    fn mastodon_timestamp_overflow_panics() {
        let ts = SnowflakeMastodonId::max_timestamp() + 1;
        SnowflakeMastodonId::from_components(ts, 0, 0);
    }

    #[test]
    #[should_panic(expected = "machine_id overflow")]
    fn mastodon_machine_id_overflow_panics() {
        let mid = SnowflakeMastodonId::max_machine_id() + 1;
        SnowflakeMastodonId::from_components(0, mid, 0);
    }

    #[test]
    #[should_panic(expected = "sequence overflow")]
    fn mastodon_sequence_overflow_panics() {
        let seq = SnowflakeMastodonId::max_sequence() + 1;
        SnowflakeMastodonId::from_components(0, 0, seq);
    }

    #[test]
    #[should_panic(expected = "timestamp overflow")]
    fn instagram_timestamp_overflow_panics() {
        let ts = SnowflakeInstagramId::max_timestamp() + 1;
        SnowflakeInstagramId::from_components(ts, 0, 0);
    }

    #[test]
    #[should_panic(expected = "machine_id overflow")]
    fn instagram_machine_id_overflow_panics() {
        let mid = SnowflakeInstagramId::max_machine_id() + 1;
        SnowflakeInstagramId::from_components(0, mid, 0);
    }

    #[test]
    #[should_panic(expected = "sequence overflow")]
    fn instagram_sequence_overflow_panics() {
        let seq = SnowflakeInstagramId::max_sequence() + 1;
        SnowflakeInstagramId::from_components(0, 0, seq);
    }

    #[test]
    #[should_panic(expected = "timestamp overflow")]
    fn long_timestamp_overflow_panics() {
        let ts = SnowflakeLongId::max_timestamp() + 1;
        SnowflakeLongId::from_components(ts, 0, 0);
    }

    #[test]
    #[should_panic(expected = "machine_id overflow")]
    fn long_machine_id_overflow_panics() {
        let mid = SnowflakeLongId::max_machine_id() + 1;
        SnowflakeLongId::from_components(0, mid, 0);
    }

    #[test]
    #[should_panic(expected = "sequence overflow")]
    fn long_sequence_overflow_panics() {
        let seq = SnowflakeLongId::max_sequence() + 1;
        SnowflakeLongId::from_components(0, 0, seq);
    }

    #[test]
    fn twitter_low_bit_fields() {
        let id = SnowflakeTwitterId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let id = SnowflakeTwitterId::from_components(1, 1, 1);
        assert_eq!(id.timestamp(), 1);
        assert_eq!(id.machine_id(), 1);
        assert_eq!(id.sequence(), 1);
    }

    #[test]
    fn discord_low_bit_fields() {
        let id = SnowflakeDiscordId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let id = SnowflakeDiscordId::from_components(1, 1, 1);
        assert_eq!(id.timestamp(), 1);
        assert_eq!(id.machine_id(), 1);
        assert_eq!(id.sequence(), 1);
    }

    #[test]
    fn mastodon_low_bit_fields() {
        let id = SnowflakeMastodonId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let id = SnowflakeMastodonId::from_components(1, 0, 1);
        assert_eq!(id.timestamp(), 1);
        assert_eq!(id.machine_id(), 0); // always zero
        assert_eq!(id.sequence(), 1);
    }

    #[test]
    fn instagram_low_bit_fields() {
        let id = SnowflakeInstagramId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let id = SnowflakeInstagramId::from_components(1, 1, 1);
        assert_eq!(id.timestamp(), 1);
        assert_eq!(id.machine_id(), 1);
        assert_eq!(id.sequence(), 1);
    }

    #[test]
    fn long_low_bit_fields() {
        let id = SnowflakeLongId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let id = SnowflakeLongId::from_components(1, 1, 1);
        assert_eq!(id.timestamp(), 1);
        assert_eq!(id.machine_id(), 1);
        assert_eq!(id.sequence(), 1);
    }

    #[test]
    fn twitter_edge_rollover() {
        let id = SnowflakeTwitterId::from_components(0, 0, SnowflakeTwitterId::max_sequence());
        assert_eq!(id.sequence(), SnowflakeTwitterId::max_sequence());

        let id = SnowflakeTwitterId::from_components(0, SnowflakeTwitterId::max_machine_id(), 0);
        assert_eq!(id.machine_id(), SnowflakeTwitterId::max_machine_id());
    }

    #[test]
    fn discord_edge_rollover() {
        let id = SnowflakeDiscordId::from_components(0, 0, SnowflakeDiscordId::max_sequence());
        assert_eq!(id.sequence(), SnowflakeDiscordId::max_sequence());

        let id = SnowflakeDiscordId::from_components(0, SnowflakeDiscordId::max_machine_id(), 0);
        assert_eq!(id.machine_id(), SnowflakeDiscordId::max_machine_id());
    }

    #[test]
    fn mastodon_edge_rollover() {
        let id = SnowflakeMastodonId::from_components(0, 0, SnowflakeMastodonId::max_sequence());
        assert_eq!(id.sequence(), SnowflakeMastodonId::max_sequence());
    }

    #[test]
    fn instagram_edge_rollover() {
        let id = SnowflakeInstagramId::from_components(0, 0, SnowflakeInstagramId::max_sequence());
        assert_eq!(id.sequence(), SnowflakeInstagramId::max_sequence());

        let id =
            SnowflakeInstagramId::from_components(0, SnowflakeInstagramId::max_machine_id(), 0);
        assert_eq!(id.machine_id(), SnowflakeInstagramId::max_machine_id());
    }

    #[test]
    fn long_edge_rollover() {
        let id = SnowflakeLongId::from_components(0, 0, SnowflakeLongId::max_sequence());
        assert_eq!(id.sequence(), SnowflakeLongId::max_sequence());

        let id = SnowflakeLongId::from_components(0, SnowflakeLongId::max_machine_id(), 0);
        assert_eq!(id.machine_id(), SnowflakeLongId::max_machine_id());
    }
}
