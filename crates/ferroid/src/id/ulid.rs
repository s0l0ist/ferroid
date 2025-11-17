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

    /// Returns `true` if the ID's internal structure is valid, such as reserved
    /// bits being unset or fields within expected ranges.
    fn is_valid(&self) -> bool;

    /// Returns a normalized version of the ID with any invalid or reserved bits
    /// cleared. This guarantees a valid, canonical representation.
    #[must_use]
    fn into_valid(self) -> Self;
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
            // Compile-time check: total bit width _must_ equal the backing
            // type. This is to avoid aliasing surprises.
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

            pub const RESERVED_MASK: $int = ((1 << Self::RESERVED_BITS) - 1);
            pub const TIMESTAMP_MASK: $int = ((1 << Self::TIMESTAMP_BITS) - 1);
            pub const RANDOM_MASK: $int = ((1 << Self::RANDOM_BITS) - 1);

            const fn valid_mask() -> $int {
                (Self::TIMESTAMP_MASK << Self::TIMESTAMP_SHIFT) |
                (Self::RANDOM_MASK << Self::RANDOM_SHIFT)
            }

            #[must_use]
            pub const fn from(timestamp: $int, random: $int) -> Self {
                let t = (timestamp & Self::TIMESTAMP_MASK) << Self::TIMESTAMP_SHIFT;
                let r = (random & Self::RANDOM_MASK) << Self::RANDOM_SHIFT;
                Self { id: t | r }
            }

            /// Extracts the timestamp from the packed ID.
            #[must_use]
            pub const fn timestamp(&self) -> $int {
                (self.id >> Self::TIMESTAMP_SHIFT) & Self::TIMESTAMP_MASK
            }
            /// Extracts the random number from the packed ID.
            #[must_use]
            pub const fn random(&self) -> $int {
                (self.id >> Self::RANDOM_SHIFT) & Self::RANDOM_MASK
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

            /// Converts a raw type into this type
            #[must_use]
            pub const fn from_raw(raw: $int) -> Self {
                Self { id: raw }
            }

            $crate::cfg_std! {
                /// Generates a non-monotonic ULID using the current system time in
                /// milliseconds since the Unix epoch and the built-in
                /// [`ThreadRandom`] random generator.
                ///
                /// This convenience constructor does **not** maintain any internal
                /// state and therefore does *not* guarantee monotonicity when
                /// multiple IDs are created within the same millisecond. If you
                /// have a bursty load or need strictly monotonic ULIDs, prefer a
                /// stateful generator such as [`BasicUlidGenerator`] or
                /// [`BasicMonoUlidGenerator`].
                ///
                /// Internally, this performs a system time query on every call,
                /// making it the slowest way to generate a ULID, but it is suitable
                /// for low-volume or one-off ID generation.
                ///
                /// [`ThreadRandom`]: crate::rand::ThreadRandom
                /// [`BasicUlidGenerator`]: crate::generator::BasicUlidGenerator
                /// [`BasicMonoUlidGenerator`]:
                ///     crate::generator::BasicMonoUlidGenerator
                #[must_use]
                pub fn now() -> Self {
                    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
                    {
                        use web_time::web::SystemTimeExt;
                        Self::from_datetime(web_time::SystemTime::now().to_std())
                    }
                    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
                    {
                        Self::from_datetime(std::time::SystemTime::now())
                    }
                }
            }

            $crate::cfg_std! {
                /// Returns this ULID's timestamp as a [`std::time::SystemTime`].
                ///
                /// The ULID timestamp encodes the number of milliseconds since
                /// [`std::time::UNIX_EPOCH`].
                ///
                /// # ⚠️ Note
                /// The precision is limited to whole milliseconds, matching the
                /// ULID specification.
                #[must_use]
                pub fn datetime(&self) -> std::time::SystemTime {
                    std::time::SystemTime::UNIX_EPOCH
                        + std::time::Duration::from_millis(self.timestamp() as u64)
                }
            }

            $crate::cfg_std! {
                /// Generates a ULID from the given timestamp in milliseconds since
                /// UNIX epoch, using the built-in [`ThreadRandom`] random
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
                Self::from(timestamp, random)
            }

            $crate::cfg_std! {
                /// Generates a ULID from the given `SystemTime`, using the built-in
                /// [`ThreadRandom`] random generator.
                ///
                /// [`ThreadRandom`]: crate::rand::ThreadRandom
                #[must_use]
                pub fn from_datetime(datetime: std::time::SystemTime) -> Self {
                    Self::from_datetime_and_rand(datetime, &$crate::rand::ThreadRandom)
                }
            }

            $crate::cfg_std! {
                /// Generates a ULID from the given `SystemTime` and a custom random
                /// number generator implementing [`RandSource`]
                ///
                /// [`RandSource`]: crate::rand::RandSource
                ///
                #[must_use]
                pub fn from_datetime_and_rand<R>(datetime: std::time::SystemTime, rng: &R) -> Self
                where
                    R: $crate::rand::RandSource<<Self as $crate::id::Id>::Ty>,
                {
                    let timestamp = datetime
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .unwrap_or(core::time::Duration::ZERO)
                        .as_millis();
                    let random = rng.rand();
                    Self::from(timestamp, random)
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
                // Random bits can frequencly overflow, but this is okay since
                // they're masked. We don't need a debug assertion here because
                // this is expected behavior. However, the timestamp should
                // never overflow.
                debug_assert!(timestamp <= Self::TIMESTAMP_MASK, "timestamp overflow");
                Self::from(timestamp, random)
            }

            fn is_valid(&self) -> bool {
                (self.to_raw() & !Self::valid_mask()) == 0
            }

            fn into_valid(self) -> Self {
                let raw = self.to_raw() & Self::valid_mask();
                Self::from_raw(raw)
            }
        }

        $crate::cfg_base32! {
            impl core::fmt::Display for $name {
                fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                    use $crate::base32::Base32UlidExt;
                    self.encode().fmt(f)
                }
            }
            impl PartialEq<str> for $name {
                fn eq(&self, other: &str) -> bool {
                    use $crate::base32::Base32UlidExt;
                    Self::decode(other).map(|id| id == *self).unwrap_or(false)
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
                        use $crate::base32::Base32UlidExt;
                        val.encode().as_string()
                    }
                }
                impl From<&$name> for $crate::__internal::String {
                    fn from(val: &$name) -> Self {
                        use $crate::base32::Base32UlidExt;
                        val.encode().as_string()
                    }
                }
            }

            impl core::convert::TryFrom<&str> for $name {
                type Error = $crate::base32::Error<$name>;

                fn try_from(s: &str) -> Result<Self, Self::Error> {
                    use $crate::base32::Base32UlidExt;
                    Self::decode(s)
                }
            }

            impl core::str::FromStr for $name {
                type Err = $crate::base32::Error<$name>;

                fn from_str(s: &str) -> Result<Self, Self::Err> {
                    use $crate::base32::Base32UlidExt;
                    Self::decode(s)
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

#[cfg(all(test, feature = "std"))]
mod tests {
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

        let id = ULID::from(ts, rand);
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
    fn ulid_from_datetime() {
        let id = ULID::from_datetime(std::time::SystemTime::UNIX_EPOCH);
        assert_eq!(id.timestamp(), 0);

        let id = ULID::from_datetime(
            std::time::SystemTime::UNIX_EPOCH + core::time::Duration::from_millis(1000),
        );
        assert_eq!(id.timestamp(), 1000);
    }

    #[test]
    fn ulid_from_datetime_and_rand() {
        let id = ULID::from_datetime_and_rand(
            std::time::SystemTime::UNIX_EPOCH + core::time::Duration::from_millis(42),
            &MockRand,
        );
        assert_eq!(id.timestamp(), 42);
        assert_eq!(id.random(), 42);
    }
}
