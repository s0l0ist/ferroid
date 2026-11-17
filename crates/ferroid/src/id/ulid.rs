use core::hash::Hash;

use crate::id::Id;

/// Trait for layout-compatible ULID-style identifiers.
///
/// This trait abstracts a `timestamp`, `random` , and `sequence` partitions
/// over a fixed-size integer (e.g., `u128`) used for high-entropy time-sortable
/// ID generation.
///
/// Types implementing `UlidId` expose methods for construction, encoding, and
/// extracting field components from packed integers.
pub trait UlidId: Id {
    /// Returns the timestamp portion of the ID.
    fn timestamp(&self) -> Self::Ty;

    /// Returns the random portion of the ID.
    fn random(&self) -> Self::Ty;

    /// Returns the maximum possible value for the timestamp field.
    fn max_timestamp() -> Self::Ty;

    /// Returns the maximum possible value for the random field.
    fn max_random() -> Self::Ty;

    /// Constructs a new ULID from its components.
    ///
    /// Macro-generated IDs mask components to their field widths. This is not
    /// fallible input validation; see the generated constructor's overflow and
    /// panic behavior. `is_valid()` cannot detect bits discarded here.
    #[must_use]
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
    #[must_use]
    fn increment_random(&self) -> Self {
        Self::from_components(self.timestamp(), self.next_random())
    }

    /// Returns a new ID for a newer timestamp with sequence reset to zero.
    #[must_use]
    fn rollover_to_timestamp(&self, ts: Self::Ty, rand: Self::Ty) -> Self {
        Self::from_components(ts, rand)
    }
}

/// A macro for defining a bit layout for a custom Ulid using three required
/// components: `reserved`, `timestamp`, and `random`.
///
/// These components are always laid out from **most significant bit (MSB)** to
/// **least significant bit (LSB)** - in that exact order.
///
/// - The first field (`reserved`) occupies the highest bits.
/// - The last field (`random`) occupies the lowest bits.
/// - The total number of bits **must exactly equal** the size of the backing
///   integer type (`u64`, `u128`, etc.). If it doesn't, the macro will trigger
///   a compile-time assertion failure.
/// - A field may be empty (zero bits) or occupy the entire backing integer.
///
/// ```text
/// define_ulid!(
///     <TypeName>, <IntegerType>,
///     reserved: <bits>,
///     timestamp: <bits>,
///     random: <bits>
/// );
/// ```
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
#[cfg_attr(docsrs, doc(cfg(feature = "ulid")))]
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
        #[repr(transparent)]
        pub struct $name {
            id: $int,
        }

        const _: () = {
            // Require the fields to cover the backing integer exactly.
            assert!(
                $reserved_bits + $timestamp_bits + $random_bits == <$int>::BITS,
                "Layout must match underlying type width"
            );
        };


        impl $name {
            pub const RESERVED_BITS: $int = $reserved_bits;
            pub const TIMESTAMP_BITS: $int = $timestamp_bits;
            pub const RANDOM_BITS: $int = $random_bits;

            pub const RANDOM_SHIFT: $int = 0;
            pub const TIMESTAMP_SHIFT: $int = Self::RANDOM_SHIFT + Self::RANDOM_BITS;
            pub const RESERVED_SHIFT: $int = Self::TIMESTAMP_SHIFT + Self::TIMESTAMP_BITS;

            // Unbounded shifts handle empty fields and fields spanning the
            // backing integer. The layout assertion bounds every count.
            pub const RESERVED_MASK: $int =
                <$int>::MAX.unbounded_shr(<$int>::BITS - Self::bit_count(Self::RESERVED_BITS));
            pub const TIMESTAMP_MASK: $int =
                <$int>::MAX.unbounded_shr(<$int>::BITS - Self::bit_count(Self::TIMESTAMP_BITS));
            pub const RANDOM_MASK: $int =
                <$int>::MAX.unbounded_shr(<$int>::BITS - Self::bit_count(Self::RANDOM_BITS));

            // Public layout constants use the backing integer for compatibility;
            // primitive shift methods take u32. The layout bounds every count.
            // Const blocks at call sites keep these checks out of runtime code
            // and preserve cross-crate inlining of the small field methods.
            #[allow(clippy::cast_possible_truncation, clippy::unnecessary_cast)]
            const fn bit_count(bits: $int) -> u32 {
                assert!(
                    bits <= <$int>::BITS as $int,
                    "bit count exceeds backing integer"
                );
                bits as u32
            }

            const fn valid_mask() -> $int {
                Self::TIMESTAMP_MASK.unbounded_shl(const { Self::bit_count(Self::TIMESTAMP_SHIFT) }) |
                Self::RANDOM_MASK.unbounded_shl(const { Self::bit_count(Self::RANDOM_SHIFT) })
            }

            /// Packs a timestamp and randomness into this layout.
            ///
            /// Both components are masked to their field widths; reserved bits
            /// stay zero. Excess random bits are always discarded, allowing
            /// full-width RNG output. With debug assertions disabled, oversized
            /// timestamps are truncated too. `is_valid()` cannot detect bits
            /// discarded during construction.
            ///
            /// # Panics
            ///
            /// With debug assertions enabled, panics if the timestamp exceeds
            /// its field's maximum. Excess random bits do not cause a panic.
            #[must_use]
            pub const fn from_components(timestamp: $int, random: $int) -> Self {
                // Mask excess random bits; full-width RNG output is expected.
                // The timestamp must still fit its field.
                debug_assert!(timestamp <= Self::TIMESTAMP_MASK, "timestamp overflow");

                let t = (timestamp & Self::TIMESTAMP_MASK)
                    .unbounded_shl(const { Self::bit_count(Self::TIMESTAMP_SHIFT) });
                let r = (random & Self::RANDOM_MASK)
                    .unbounded_shl(const { Self::bit_count(Self::RANDOM_SHIFT) });
                Self { id: t | r }
            }

            /// Extracts the timestamp from the packed ID.
            #[must_use]
            pub const fn timestamp(&self) -> $int {
                self.id.unbounded_shr(const { Self::bit_count(Self::TIMESTAMP_SHIFT) })
                    & Self::TIMESTAMP_MASK
            }
            /// Extracts the random number from the packed ID.
            #[must_use]
            pub const fn random(&self) -> $int {
                self.id.unbounded_shr(const { Self::bit_count(Self::RANDOM_SHIFT) })
                    & Self::RANDOM_MASK
            }
            /// Returns the maximum representable timestamp value based on
            /// `Self::TIMESTAMP_BITS`.
            #[must_use]
            pub const fn max_timestamp() -> $int {
                Self::TIMESTAMP_MASK
            }
            /// Returns the maximum representable randome value based on
            /// `Self::RANDOM_BIT`.
            #[must_use]
            pub const fn max_random() -> $int {
                Self::RANDOM_MASK
            }

            /// Converts this type into its raw type representation
            #[must_use]
            pub const fn to_raw(&self) -> $int {
                self.id
            }

            /// Wraps a packed integer without validating reserved bits.
            ///
            /// Use [`TryFrom`] for checked conversion from the backing integer.
            ///
            /// All input bits are preserved, including reserved ones. Passing
            /// the backing integer's maximum is invalid if the layout reserves
            /// any bits. For untrusted values, use
            /// `is_valid()` to check the layout, or
            /// `into_valid()` to clear reserved bits and change the raw value.
            #[must_use]
            pub const fn from_raw(raw: $int) -> Self {
                Self { id: raw }
            }

            /// Returns `true` when this ID's reserved bits are zero.
            ///
            /// This cannot detect component values truncated during construction.
            #[must_use]
            pub const fn is_valid(&self) -> bool {
                (self.id & !Self::valid_mask()) == 0
            }

            /// Clears reserved bits, which can change the ID's raw value.
            #[must_use]
            pub const fn into_valid(self) -> Self {
                Self::from_raw(self.id & Self::valid_mask())
            }

            $crate::cfg_thread_rng! {
                /// Generates a non-monotonic ULID using the current system time
                /// in milliseconds since the Unix epoch and the built-in
                /// [`ThreadRandom`] random generator.
                ///
                /// Samples fresh randomness on every call, without retaining
                /// the previous ID. IDs generated within the same millisecond
                /// have no guaranteed ordering. For strictly increasing IDs,
                /// use a monotonic generator such as [`BasicMonoUlidGenerator`].
                ///
                /// Reads wall time on every call. For the built-in `ULID`,
                /// `thread_local::direct::Ulid::new_ulid()` delegates here.
                ///
                /// [`ThreadRandom`]: crate::rand::ThreadRandom
                /// [`BasicMonoUlidGenerator`]:
                ///     crate::generator::BasicMonoUlidGenerator
                #[must_use]
                pub fn now() -> Self {
                    Self::from_system_time($crate::__internal::system_time_now())
                }
            }

            $crate::cfg_std! {
                /// Returns this ULID's timestamp as a
                /// [`std::time::SystemTime`].
                ///
                /// The ULID timestamp encodes the number of milliseconds since
                /// [`std::time::UNIX_EPOCH`].
                ///
                /// # ⚠️ Note
                /// The precision is limited to whole milliseconds, matching the
                /// ULID specification. Timestamp values exceeding `u64::MAX`
                /// will be truncated.
                #[must_use]
                #[allow(clippy::cast_possible_truncation)]
                pub fn system_time(&self) -> $crate::__internal::SystemTime {
                    $crate::__internal::SystemTime::UNIX_EPOCH
                        + core::time::Duration::from_millis(self.timestamp() as u64)
                }
            }

            $crate::cfg_thread_rng! {
                /// Generates a ULID from the given timestamp in milliseconds
                /// since UNIX epoch, using the built-in [`ThreadRandom`] random
                /// generator.
                ///
                /// [`ThreadRandom`]: crate::rand::ThreadRandom
                #[must_use]
                pub fn from_timestamp(timestamp: <Self as $crate::id::Id>::Ty) -> Self {
                    Self::from_timestamp_and_rand(timestamp, &$crate::rand::ThreadRandom)
                }
            }

            /// Generates a ULID from the given timestamp in milliseconds since
            /// UNIX epoch and a custom random number generator implementing
            /// [`RandSource`]
            ///
            /// [`RandSource`]: crate::rand::RandSource
            #[must_use]
            pub fn from_timestamp_and_rand<R>(timestamp: <Self as $crate::id::Id>::Ty, rng: &R) -> Self
            where
                R: $crate::rand::RandSource<<Self as $crate::id::Id>::Ty>,
            {
                let random = rng.rand();
                Self::from_components(timestamp, random)
            }

            $crate::cfg_thread_rng! {
                /// Generates a ULID from the given `SystemTime`, using the
                /// built-in [`ThreadRandom`] random generator.
                ///
                /// [`ThreadRandom`]: crate::rand::ThreadRandom
                #[must_use]
                pub fn from_system_time(system_time: $crate::__internal::SystemTime) -> Self {
                    Self::from_system_time_and_rand(system_time, &$crate::rand::ThreadRandom)
                }
            }

            $crate::cfg_std! {
                /// Generates a ULID from the given `SystemTime` and a custom
                /// random number generator implementing [`RandSource`]
                ///
                /// [`RandSource`]: crate::rand::RandSource
                ///
                #[must_use]
                pub fn from_system_time_and_rand<R>(system_time: $crate::__internal::SystemTime, rng: &R) -> Self
                where
                    R: $crate::rand::RandSource<<Self as $crate::id::Id>::Ty>,
                {
                    let timestamp = system_time
                        .duration_since($crate::__internal::SystemTime::UNIX_EPOCH)
                        .unwrap_or(core::time::Duration::ZERO)
                        .as_millis();
                    let random = rng.rand();
                    Self::from_components(timestamp, random)
                }
            }
        }

        impl $crate::id::Id for $name {
            type Ty = $int;
            const ZERO: $int = 0;
            const ONE: $int = 1;

            /// Converts this type into its raw type representation
            fn to_raw(&self) -> Self::Ty {
                self.to_raw()
            }

            /// Converts a raw type into this type
            fn from_raw(raw: Self::Ty) -> Self {
                Self::from_raw(raw)
            }

            fn is_valid(&self) -> bool {
                self.is_valid()
            }

            fn into_valid(self) -> Self {
                self.into_valid()
            }
        }

        impl core::convert::TryFrom<$int> for $name {
            type Error = $crate::Error<Self>;

            fn try_from(raw: $int) -> core::result::Result<Self, Self::Error> {
                let id = Self::from_raw(raw);
                if id.is_valid() {
                    Ok(id)
                } else {
                    Err($crate::Error::DecodeOverflow { id })
                }
            }
        }

        impl $crate::id::UlidId for $name {
            fn timestamp(&self) -> Self::Ty {
                self.timestamp()
            }

            fn random(&self) -> Self::Ty {
                self.random()
            }

            fn max_timestamp() -> Self::Ty {
                Self::TIMESTAMP_MASK
            }

            fn max_random() -> Self::Ty {
                Self::RANDOM_MASK
            }

            fn from_components(timestamp: $int, random: $int) -> Self {
                Self::from_components(timestamp, random)
            }
        }

        $crate::cfg_base32! {
            impl core::fmt::Display for $name {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    $crate::base32::Base32Ext::encode(self).fmt(f)
                }
            }
            impl PartialEq<str> for $name {
                fn eq(&self, other: &str) -> bool {
                    <Self as $crate::base32::Base32Ext>::decode(other).map(|id| id == *self).unwrap_or(false)
                }
            }
            impl PartialEq<&str> for $name {
                fn eq(&self, other: &&str) -> bool {
                    self == *other
                }
            }
            impl PartialEq<$name> for &str {
                fn eq(&self, other: &$name) -> bool {
                    other == *self
                }
            }

            $crate::cfg_alloc! {
                impl PartialEq<$crate::__internal::String> for $name {
                    fn eq(&self, other: &$crate::__internal::String) -> bool {
                        self == other.as_str()
                    }
                }
                impl PartialEq<$name> for $crate::__internal::String {
                    fn eq(&self, other: &$name) -> bool {
                        other == self
                    }
                }
                impl From<$name> for $crate::__internal::String {
                    fn from(val: $name) -> Self {
                        $crate::base32::Base32Ext::encode(&val).as_string()
                    }
                }
                impl From<&$name> for $crate::__internal::String {
                    fn from(val: &$name) -> Self {
                        $crate::base32::Base32Ext::encode(val).as_string()
                    }
                }
            }

            impl core::convert::TryFrom<&str> for $name {
                type Error = $crate::base32::Error<$name>;

                fn try_from(s: &str) -> Result<Self, Self::Error> {
                    <Self as $crate::base32::Base32Ext>::decode(s)
                }
            }

            impl core::str::FromStr for $name {
                type Err = $crate::base32::Error<$name>;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    <Self as $crate::base32::Base32Ext>::decode(s)
                }
            }
        }

        impl core::fmt::Debug for $name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                let full = core::any::type_name::<Self>();
                let name = full.rsplit("::").next().unwrap_or(full);
                let mut dbg = f.debug_struct(name);
                dbg.field("id", &format_args!("{:} (0x{:x})", self.to_raw(), self.to_raw()));
                dbg.field("timestamp", &format_args!("{:} (0x{:x})", self.timestamp(), self.timestamp()));
                dbg.field("random", &format_args!("{:} (0x{:x})", self.random(), self.random()));
                dbg.finish()
            }
        }
    };
}

define_ulid!(
    /// A 128-bit ULID
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

    #[test]
    fn checked_raw_conversion_accepts_the_full_ulid_range() {
        for raw in [0u128, 42, u128::MAX] {
            assert_eq!(ULID::try_from(raw).unwrap().to_raw(), raw);
            let converted: ULID = raw.try_into().unwrap();
            assert_eq!(converted.to_raw(), raw);
        }
        #[cfg(feature = "base32")]
        assert_eq!(
            ULID::try_from("0000000000000000000000001A")
                .unwrap()
                .to_raw(),
            42
        );
    }

    #[test]
    #[allow(
        dead_code,
        clippy::wrong_self_convention,
        reason = "The layout fixture generates a complete ID API; this test checks raw conversion."
    )]
    fn checked_raw_conversion_validates_custom_ulid_reserved_bits() {
        crate::define_ulid!(CustomId, u128, reserved: 1, timestamp: 47, random: 80);
        assert_eq!(CustomId::try_from(0u128).unwrap().to_raw(), 0);
        assert_eq!(
            CustomId::try_from(u128::MAX >> 1).unwrap().to_raw(),
            u128::MAX >> 1
        );
        assert_eq!(
            CustomId::try_from(u128::MAX),
            Err(crate::Error::DecodeOverflow {
                id: CustomId::from_raw(u128::MAX)
            })
        );
        let converted: Result<CustomId, _> = u128::MAX.try_into();
        assert_eq!(
            converted,
            Err(crate::Error::DecodeOverflow {
                id: CustomId::from_raw(u128::MAX)
            })
        );
    }
    use std::println;

    use super::*;
    use crate::rand::RandSource;

    struct MockRand;
    impl RandSource<u128> for MockRand {
        fn rand(&self) -> u128 {
            42
        }
    }

    #[test]
    #[allow(
        dead_code,
        clippy::wrong_self_convention,
        reason = "Layout fixtures generate complete ID APIs; these tests exercise field boundaries."
    )]
    fn timestamp_can_fill_u128() {
        define_ulid!(TimestampOnly, u128, reserved: 0, timestamp: 128, random: 0);

        const ID: TimestampOnly = TimestampOnly::from_components(u128::MAX, 0);
        const _: () = assert!(ID.timestamp() == u128::MAX);

        assert_eq!(ID.to_raw(), u128::MAX);
        assert_eq!((ID.timestamp(), ID.random()), (u128::MAX, 0));
        assert!(ID.is_valid());
    }

    #[test]
    #[allow(
        dead_code,
        clippy::wrong_self_convention,
        reason = "Layout fixtures generate complete ID APIs; these tests exercise field boundaries."
    )]
    fn random_can_fill_u128() {
        define_ulid!(RandomOnly, u128, reserved: 0, timestamp: 0, random: 128);

        const ID: RandomOnly = RandomOnly::from_components(0, u128::MAX);
        const _: () = assert!(ID.random() == u128::MAX);

        assert_eq!(ID.to_raw(), u128::MAX);
        assert_eq!((ID.timestamp(), ID.random()), (0, u128::MAX));
        assert!(ID.is_valid());
        assert_eq!(ID.into_valid(), ID);
    }

    #[test]
    #[allow(
        dead_code,
        clippy::wrong_self_convention,
        reason = "Layout fixtures generate complete ID APIs; these tests exercise field boundaries."
    )]
    fn fully_reserved_layout_only_accepts_zero() {
        define_ulid!(ReservedOnly, u128, reserved: 128, timestamp: 0, random: 0);

        const ZERO: ReservedOnly = ReservedOnly::from_components(0, 0);
        let reserved = ReservedOnly::from_raw(u128::MAX);
        assert_eq!(ReservedOnly::try_from(0u128).unwrap().to_raw(), 0);
        assert_eq!(
            ReservedOnly::try_from(1u128),
            Err(crate::Error::DecodeOverflow {
                id: ReservedOnly::from_raw(1u128)
            })
        );

        assert_eq!(ReservedOnly::RESERVED_MASK, u128::MAX);
        assert_eq!(ZERO.to_raw(), 0);
        assert!(ZERO.is_valid());
        assert!(!reserved.is_valid());
        assert_eq!(reserved.into_valid(), ZERO);
        assert_eq!((reserved.timestamp(), reserved.random()), (0, 0));
    }
    #[test]
    fn ulid_validity() {
        let id = ULID::from_raw(u128::MAX);
        assert!(id.is_valid());
        let valid = id.into_valid();
        assert!(valid.is_valid());
    }

    #[test]
    fn test_ulid_id_fields_and_bounds() {
        let ts = ULID::max_timestamp();
        let rand = ULID::max_random();

        let id = ULID::from_components(ts, rand);
        println!("ID: {id:#?}");
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

    #[test]
    #[cfg(feature = "thread-rng")]
    fn ulid_from_timestamp() {
        let id = ULID::from_timestamp(0);
        assert_eq!(id.timestamp(), 0);

        let id = ULID::from_timestamp(ULID::max_timestamp());
        assert_eq!(id.timestamp(), ULID::max_timestamp());
    }

    #[test]
    fn ulid_from_timestamp_and_rand() {
        let id = ULID::from_timestamp_and_rand(42, &MockRand);
        assert_eq!(id.timestamp(), 42);
        assert_eq!(id.random(), 42);
    }

    #[test]
    #[cfg(feature = "thread-rng")]
    fn ulid_from_system_time() {
        let id = ULID::from_system_time(std::time::SystemTime::UNIX_EPOCH);
        assert_eq!(id.timestamp(), 0);

        let id = ULID::from_system_time(
            std::time::SystemTime::UNIX_EPOCH + core::time::Duration::from_secs(1),
        );
        assert_eq!(id.timestamp(), 1000);
    }

    #[test]
    #[cfg(feature = "std")]
    fn ulid_from_system_time_and_rand() {
        let id = ULID::from_system_time_and_rand(
            std::time::SystemTime::UNIX_EPOCH + core::time::Duration::from_millis(42),
            &MockRand,
        );
        assert_eq!(id.timestamp(), 42);
        assert_eq!(id.random(), 42);
    }
}
