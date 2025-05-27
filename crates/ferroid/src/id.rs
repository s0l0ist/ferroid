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
    Sized + Copy + Clone + fmt::Display + PartialOrd + Ord + PartialEq + Eq + Hash
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnowflakeTwitterId {
    id: u64,
}

impl SnowflakeTwitterId {
    /// Bitmask for extracting the 41-bit timestamp field. Occupies bits 22
    /// through 62.
    pub const TIMESTAMP_MASK: u64 = (1 << 41) - 1;

    /// Bitmask for extracting the 10-bit machine ID field. Occupies bits 12
    /// through 21.
    pub const MACHINE_ID_MASK: u64 = (1 << 10) - 1;

    /// Bitmask for extracting the 12-bit sequence field. Occupies bits 0
    /// through 11.
    pub const SEQUENCE_MASK: u64 = (1 << 12) - 1;

    /// Number of bits to shift the timestamp to its correct position (bit 23).
    pub const TIMESTAMP_SHIFT: u64 = 22;

    /// Number of bits to shift the machine ID to its correct position (bit 10).
    pub const MACHINE_ID_SHIFT: u64 = 12;

    /// Number of bits to shift the sequence field (bit 0).
    pub const SEQUENCE_SHIFT: u64 = 0;

    pub const fn from(timestamp: u64, machine_id: u64, sequence: u64) -> Self {
        let timestamp = (timestamp & Self::TIMESTAMP_MASK) << Self::TIMESTAMP_SHIFT;
        let machine_id = (machine_id & Self::MACHINE_ID_MASK) << Self::MACHINE_ID_SHIFT;
        let sequence = (sequence & Self::SEQUENCE_MASK) << Self::SEQUENCE_SHIFT;
        Self {
            id: timestamp | machine_id | sequence,
        }
    }

    /// Extracts the timestamp from the packed ID.
    pub const fn timestamp(&self) -> u64 {
        (self.id >> Self::TIMESTAMP_SHIFT) & Self::TIMESTAMP_MASK
    }

    /// Extracts the machine ID from the packed ID.
    pub const fn machine_id(&self) -> u64 {
        (self.id >> Self::MACHINE_ID_SHIFT) & Self::MACHINE_ID_MASK
    }

    /// Extracts the sequence number from the packed ID.
    pub const fn sequence(&self) -> u64 {
        (self.id >> Self::SEQUENCE_SHIFT) & Self::SEQUENCE_MASK
    }

    /// Returns the ID as a zero-padded 20-digit string.
    pub fn to_padded_string(&self) -> String {
        format!("{:020}", self.id)
    }
}

impl Snowflake for SnowflakeTwitterId {
    type Ty = u64;

    const ZERO: Self::Ty = 0;
    const ONE: Self::Ty = 1;

    fn timestamp(&self) -> Self::Ty {
        self.timestamp()
    }

    fn max_timestamp() -> Self::Ty {
        Self::TIMESTAMP_MASK
    }

    fn machine_id(&self) -> Self::Ty {
        self.machine_id()
    }

    fn max_machine_id() -> Self::Ty {
        Self::MACHINE_ID_MASK
    }

    fn sequence(&self) -> Self::Ty {
        self.sequence()
    }

    fn max_sequence() -> Self::Ty {
        Self::SEQUENCE_MASK
    }

    fn from_components(timestamp: Self::Ty, machine_id: Self::Ty, sequence: Self::Ty) -> Self {
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
        self.to_padded_string()
    }
}

impl fmt::Display for SnowflakeTwitterId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl fmt::Debug for SnowflakeTwitterId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or("Unknown");
        write_bit_layout_debug(f, self, name)
    }
}

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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnowflakeDiscordId {
    id: u64,
}

impl SnowflakeDiscordId {
    /// Bitmask for extracting the 42-bit timestamp field. Occupies bits 22
    /// through 63.
    pub const TIMESTAMP_MASK: u64 = (1 << 42) - 1;

    /// Bitmask for extracting the 10-bit machine ID field. Occupies bits 12
    /// through 21.
    pub const MACHINE_ID_MASK: u64 = (1 << 10) - 1;

    /// Bitmask for extracting the 12-bit sequence field. Occupies bits 0
    /// through 11.
    pub const SEQUENCE_MASK: u64 = (1 << 12) - 1;

    /// Number of bits to shift the timestamp to its correct position (bit 22).
    pub const TIMESTAMP_SHIFT: u64 = 22;

    /// Number of bits to shift the machine ID to its correct position (bit 12).
    pub const MACHINE_ID_SHIFT: u64 = 12;

    /// Number of bits to shift the sequence field (bit 0).
    pub const SEQUENCE_SHIFT: u64 = 0;

    pub const fn from(timestamp: u64, machine_id: u64, sequence: u64) -> Self {
        let timestamp = (timestamp & Self::TIMESTAMP_MASK) << Self::TIMESTAMP_SHIFT;
        let machine_id = (machine_id & Self::MACHINE_ID_MASK) << Self::MACHINE_ID_SHIFT;
        let sequence = (sequence & Self::SEQUENCE_MASK) << Self::SEQUENCE_SHIFT;
        Self {
            id: timestamp | machine_id | sequence,
        }
    }

    /// Extracts the timestamp from the packed ID.
    pub const fn timestamp(&self) -> u64 {
        (self.id >> Self::TIMESTAMP_SHIFT) & Self::TIMESTAMP_MASK
    }

    /// Extracts the machine ID from the packed ID.
    pub const fn machine_id(&self) -> u64 {
        (self.id >> Self::MACHINE_ID_SHIFT) & Self::MACHINE_ID_MASK
    }

    /// Extracts the sequence number from the packed ID.
    pub const fn sequence(&self) -> u64 {
        (self.id >> Self::SEQUENCE_SHIFT) & Self::SEQUENCE_MASK
    }

    /// Returns the ID as a zero-padded 20-digit string.
    pub fn to_padded_string(&self) -> String {
        format!("{:020}", self.id)
    }
}

impl Snowflake for SnowflakeDiscordId {
    type Ty = u64;
    const ZERO: Self::Ty = 0;
    const ONE: Self::Ty = 1;

    fn timestamp(&self) -> Self::Ty {
        self.timestamp()
    }

    fn max_timestamp() -> Self::Ty {
        Self::TIMESTAMP_MASK
    }

    fn machine_id(&self) -> Self::Ty {
        self.machine_id()
    }

    fn max_machine_id() -> Self::Ty {
        Self::MACHINE_ID_MASK
    }

    fn sequence(&self) -> Self::Ty {
        self.sequence()
    }

    fn max_sequence() -> Self::Ty {
        Self::SEQUENCE_MASK
    }

    fn from_components(timestamp: Self::Ty, machine_id: Self::Ty, sequence: Self::Ty) -> Self {
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
        self.to_padded_string()
    }
}

impl fmt::Display for SnowflakeDiscordId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl fmt::Debug for SnowflakeDiscordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or("Unknown");
        write_bit_layout_debug(f, self, name)
    }
}

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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnowflakeMastodonId {
    id: u64,
}

impl SnowflakeMastodonId {
    /// Bitmask for extracting the 48-bit timestamp field. Occupies bits 16
    /// through 63.
    pub const TIMESTAMP_MASK: u64 = (1 << 48) - 1;

    /// Bitmask for extracting the 16-bit sequence field. Occupies bits 0
    /// through 15.
    pub const SEQUENCE_MASK: u64 = (1 << 16) - 1;

    /// Number of bits to shift the timestamp to its correct position (bit 16).
    pub const TIMESTAMP_SHIFT: u64 = 16;

    /// Number of bits to shift the sequence field (bit 0).
    pub const SEQUENCE_SHIFT: u64 = 0;

    pub const fn from(timestamp: u64, sequence: u64) -> Self {
        let timestamp = (timestamp & Self::TIMESTAMP_MASK) << Self::TIMESTAMP_SHIFT;
        let sequence = (sequence & Self::SEQUENCE_MASK) << Self::SEQUENCE_SHIFT;
        Self {
            id: timestamp | sequence,
        }
    }

    /// Extracts the timestamp from the packed ID.
    pub const fn timestamp(&self) -> u64 {
        (self.id >> Self::TIMESTAMP_SHIFT) & Self::TIMESTAMP_MASK
    }

    /// Extracts the sequence number from the packed ID.
    pub const fn sequence(&self) -> u64 {
        (self.id >> Self::SEQUENCE_SHIFT) & Self::SEQUENCE_MASK
    }

    /// Returns the ID as a zero-padded 20-digit string.
    pub fn to_padded_string(&self) -> String {
        format!("{:020}", self.id)
    }
}

impl Snowflake for SnowflakeMastodonId {
    type Ty = u64;
    const ZERO: Self::Ty = 0;
    const ONE: Self::Ty = 1;

    fn timestamp(&self) -> Self::Ty {
        self.timestamp()
    }

    fn max_timestamp() -> Self::Ty {
        Self::TIMESTAMP_MASK
    }

    fn machine_id(&self) -> Self::Ty {
        0
    }

    fn max_machine_id() -> Self::Ty {
        0
    }

    fn sequence(&self) -> Self::Ty {
        self.sequence()
    }

    fn max_sequence() -> Self::Ty {
        Self::SEQUENCE_MASK
    }

    fn from_components(timestamp: Self::Ty, _machine_id: Self::Ty, sequence: Self::Ty) -> Self {
        debug_assert!(timestamp <= Self::TIMESTAMP_MASK, "timestamp overflow");
        debug_assert!(sequence <= Self::SEQUENCE_MASK, "sequence overflow");
        Self::from(timestamp, sequence)
    }

    fn to_raw(&self) -> Self::Ty {
        self.id
    }

    fn from_raw(raw: Self::Ty) -> Self {
        Self { id: raw }
    }

    fn to_padded_string(&self) -> String {
        self.to_padded_string()
    }
}

impl fmt::Display for SnowflakeMastodonId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl fmt::Debug for SnowflakeMastodonId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or("Unknown");
        write_bit_layout_debug(f, self, name)
    }
}

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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnowflakeInstagramId {
    id: u64,
}

impl SnowflakeInstagramId {
    /// Bitmask for extracting the 41-bit timestamp field. Occupies bits 23
    /// through 63.
    pub const TIMESTAMP_MASK: u64 = (1 << 41) - 1;

    /// Bitmask for extracting the 13-bit machine ID field. Occupies bits 10
    /// through 22.
    pub const MACHINE_ID_MASK: u64 = (1 << 13) - 1;

    /// Bitmask for extracting the 10-bit sequence field. Occupies bits 0
    /// through 9.
    pub const SEQUENCE_MASK: u64 = (1 << 10) - 1;

    /// Number of bits to shift the timestamp to its correct position (bit 23).
    pub const TIMESTAMP_SHIFT: u64 = 23;

    /// Number of bits to shift the machine ID to its correct position (bit 10).
    pub const MACHINE_ID_SHIFT: u64 = 10;

    /// Number of bits to shift the sequence field (bit 0).
    pub const SEQUENCE_SHIFT: u64 = 0;

    pub const fn from(timestamp: u64, machine_id: u64, sequence: u64) -> Self {
        let timestamp = (timestamp & Self::TIMESTAMP_MASK) << Self::TIMESTAMP_SHIFT;
        let machine_id = (machine_id & Self::MACHINE_ID_MASK) << Self::MACHINE_ID_SHIFT;
        let sequence = (sequence & Self::SEQUENCE_MASK) << Self::SEQUENCE_SHIFT;
        Self {
            id: timestamp | machine_id | sequence,
        }
    }

    /// Extracts the timestamp from the packed ID.
    pub const fn timestamp(&self) -> u64 {
        (self.id >> Self::TIMESTAMP_SHIFT) & Self::TIMESTAMP_MASK
    }

    /// Extracts the machine ID from the packed ID.
    pub const fn machine_id(&self) -> u64 {
        (self.id >> Self::MACHINE_ID_SHIFT) & Self::MACHINE_ID_MASK
    }

    /// Extracts the sequence number from the packed ID.
    pub const fn sequence(&self) -> u64 {
        (self.id >> Self::SEQUENCE_SHIFT) & Self::SEQUENCE_MASK
    }

    /// Returns the ID as a zero-padded 20-digit string.
    pub fn to_padded_string(&self) -> String {
        format!("{:020}", self.id)
    }
}

impl Snowflake for SnowflakeInstagramId {
    type Ty = u64;
    const ZERO: Self::Ty = 0;
    const ONE: Self::Ty = 1;

    fn timestamp(&self) -> Self::Ty {
        self.timestamp()
    }

    fn max_timestamp() -> Self::Ty {
        Self::TIMESTAMP_MASK
    }

    fn machine_id(&self) -> Self::Ty {
        self.machine_id()
    }

    fn max_machine_id() -> Self::Ty {
        Self::MACHINE_ID_MASK
    }

    fn sequence(&self) -> Self::Ty {
        self.sequence()
    }

    fn max_sequence() -> Self::Ty {
        Self::SEQUENCE_MASK
    }

    fn from_components(timestamp: Self::Ty, machine_id: Self::Ty, sequence: Self::Ty) -> Self {
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

    fn increment_sequence(&self) -> Self {
        Self::from_components(self.timestamp(), self.machine_id(), self.next_sequence())
    }

    fn rollover_to_timestamp(&self, ts: Self::Ty) -> Self {
        Self::from_components(ts, self.machine_id(), Self::ZERO)
    }

    fn to_padded_string(&self) -> String {
        self.to_padded_string()
    }
}

impl fmt::Display for SnowflakeInstagramId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl fmt::Debug for SnowflakeInstagramId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = std::any::type_name::<Self>()
            .rsplit("::")
            .next()
            .unwrap_or("Unknown");
        write_bit_layout_debug(f, self, name)
    }
}

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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnowflakeLongId {
    id: u128,
}

impl SnowflakeLongId {
    /// Bitmask for extracting the 48-bit timestamp field. Occupies bits 40
    /// through 87.
    pub const TIMESTAMP_MASK: u128 = (1 << 48) - 1;

    /// Bitmask for extracting the 20-bit machine ID field. Occupies bits 20
    /// through 39.
    pub const MACHINE_ID_MASK: u128 = (1 << 20) - 1;

    /// Bitmask for extracting the 20-bit sequence field. Occupies bits 0
    /// through 19.
    pub const SEQUENCE_MASK: u128 = (1 << 20) - 1;

    /// Number of bits to shift the timestamp to its correct position (bit 40).
    pub const TIMESTAMP_SHIFT: u128 = 40;

    /// Number of bits to shift the machine ID to its correct position (bit 20).
    pub const MACHINE_ID_SHIFT: u128 = 20;

    /// Number of bits to shift the sequence field (bit 0).
    pub const SEQUENCE_SHIFT: u128 = 0;

    pub const fn from(timestamp: u128, machine_id: u128, sequence: u128) -> Self {
        let timestamp = (timestamp & Self::TIMESTAMP_MASK) << Self::TIMESTAMP_SHIFT;
        let machine_id = (machine_id & Self::MACHINE_ID_MASK) << Self::MACHINE_ID_SHIFT;
        let sequence = (sequence & Self::SEQUENCE_MASK) << Self::SEQUENCE_SHIFT;
        Self {
            id: timestamp | machine_id | sequence,
        }
    }

    /// Extracts the timestamp from the packed ID.
    pub const fn timestamp(&self) -> u128 {
        (self.id >> Self::TIMESTAMP_SHIFT) & Self::TIMESTAMP_MASK
    }

    /// Extracts the machine ID from the packed ID.
    pub const fn machine_id(&self) -> u128 {
        (self.id >> Self::MACHINE_ID_SHIFT) & Self::MACHINE_ID_MASK
    }

    /// Extracts the sequence number from the packed ID.
    pub const fn sequence(&self) -> u128 {
        (self.id >> Self::SEQUENCE_SHIFT) & Self::SEQUENCE_MASK
    }

    /// Returns the ID as a zero-padded 39-digit string.
    pub fn to_padded_string(&self) -> String {
        format!("{:039}", self.id)
    }
}

impl Snowflake for SnowflakeLongId {
    type Ty = u128;
    const ZERO: Self::Ty = 0;
    const ONE: Self::Ty = 1;

    fn timestamp(&self) -> Self::Ty {
        self.timestamp()
    }

    fn max_timestamp() -> Self::Ty {
        Self::TIMESTAMP_MASK
    }

    fn machine_id(&self) -> Self::Ty {
        self.machine_id()
    }

    fn max_machine_id() -> Self::Ty {
        Self::MACHINE_ID_MASK
    }

    fn sequence(&self) -> Self::Ty {
        self.sequence()
    }

    fn max_sequence() -> Self::Ty {
        Self::SEQUENCE_MASK
    }

    fn from_components(timestamp: Self::Ty, machine_id: Self::Ty, sequence: Self::Ty) -> Self {
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

    fn increment_sequence(&self) -> Self {
        Self::from_components(self.timestamp(), self.machine_id(), self.next_sequence())
    }

    fn rollover_to_timestamp(&self, ts: Self::Ty) -> Self {
        Self::from_components(ts, self.machine_id(), Self::ZERO)
    }

    fn to_padded_string(&self) -> String {
        self.to_padded_string()
    }
}

impl fmt::Display for SnowflakeLongId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl fmt::Debug for SnowflakeLongId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct(std::any::type_name::<Self>())
            .field("id", &self.id)
            .field("timestamp", &self.timestamp())
            .field("machine_id", &self.machine_id())
            .field("sequence", &self.sequence())
            .finish()
    }
}

pub struct FieldLayout {
    pub name: &'static str,
    pub bits: u8,
    pub value: u64,
}

pub trait SnowflakeBitLayout {
    fn id(&self) -> u64;
    fn fields(&self) -> Vec<FieldLayout>;
    fn to_padded_string(&self) -> String;
    #[cfg(feature = "base32")]
    fn encode(&self) -> String;
}

pub fn write_bit_layout_debug(
    f: &mut fmt::Formatter<'_>,
    id_type: &impl SnowflakeBitLayout,
    type_name: &str,
) -> fmt::Result {
    let visible_fields: Vec<_> = id_type
        .fields()
        .into_iter()
        .filter(|field| field.bits > 0)
        .collect();

    // Compute max width per column: label, dec, hex
    let columns: Vec<usize> = visible_fields
        .iter()
        .map(|field| {
            let label_len = format!("{} ({})", field.name, field.bits).len();
            let dec_len = field.value.to_string().len();
            let hex_len = format!("0x{:x}", field.value).len();
            *[label_len, dec_len, hex_len].iter().max().unwrap() + 2 // +2 for padding
        })
        .collect();

    fn center(s: impl ToString, width: usize) -> String {
        let s = s.to_string();
        let len = s.len();
        if len >= width {
            return s;
        }
        let pad = width - len;
        let left = pad / 2;
        let right = pad - left;
        format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
    }

    writeln!(f, "{} {{", type_name)?;
    writeln!(
        f,
        "    raw id     : 0x{:016x} ({})",
        id_type.id(),
        id_type.id()
    )?;
    writeln!(f, "    padded     : {}", id_type.to_padded_string())?;

    #[cfg(feature = "base32")]
    {
        writeln!(f, "    base32     : {}", id_type.encode())?;
    }

    writeln!(f, "    layout     :")?;

    // Top border
    write!(f, "        +")?;
    for &w in &columns {
        write!(f, "{}+", "-".repeat(w))?;
    }
    writeln!(f)?;

    // Field labels
    write!(f, "        |")?;
    for (field, &w) in visible_fields.iter().zip(&columns) {
        let label = format!("{} ({})", field.name, field.bits);
        write!(f, "{}|", center(label, w))?;
    }
    writeln!(f)?;

    // Mid border
    write!(f, "        +")?;
    for &w in &columns {
        write!(f, "{}+", "-".repeat(w))?;
    }
    writeln!(f)?;

    // Decimal values
    write!(f, "        |")?;
    for (field, &w) in visible_fields.iter().zip(&columns) {
        write!(f, "{}|", center(field.value, w))?;
    }
    writeln!(f)?;

    // Hex values
    write!(f, "        |")?;
    for (field, &w) in visible_fields.iter().zip(&columns) {
        write!(f, "{}|", center(format!("0x{:x}", field.value), w))?;
    }
    writeln!(f)?;

    // Bottom border
    write!(f, "        +")?;
    for &w in &columns {
        write!(f, "{}+", "-".repeat(w))?;
    }
    writeln!(f)?;

    write!(f, "}}")
}

impl SnowflakeBitLayout for SnowflakeTwitterId {
    fn id(&self) -> u64 {
        self.id
    }

    fn fields(&self) -> Vec<FieldLayout> {
        vec![
            FieldLayout {
                name: "reserved",
                bits: 1,
                value: 0,
            },
            FieldLayout {
                name: "timestamp",
                bits: 41,
                value: self.timestamp(),
            },
            FieldLayout {
                name: "machine_id",
                bits: 10,
                value: self.machine_id(),
            },
            FieldLayout {
                name: "sequence",
                bits: 12,
                value: self.sequence(),
            },
        ]
    }

    #[cfg(feature = "base32")]
    fn encode(&self) -> String {
        use crate::SnowflakeBase32Ext;
        <Self as SnowflakeBase32Ext>::encode(self)
    }

    fn to_padded_string(&self) -> String {
        self.to_padded_string()
    }
}

impl SnowflakeBitLayout for SnowflakeDiscordId {
    fn id(&self) -> u64 {
        self.id
    }

    fn fields(&self) -> Vec<FieldLayout> {
        vec![
            FieldLayout {
                name: "timestamp",
                bits: 42,
                value: self.timestamp(),
            },
            FieldLayout {
                name: "machine_id",
                bits: 10,
                value: self.machine_id(),
            },
            FieldLayout {
                name: "sequence",
                bits: 12,
                value: self.sequence(),
            },
        ]
    }

    #[cfg(feature = "base32")]
    fn encode(&self) -> String {
        use crate::SnowflakeBase32Ext;
        <Self as SnowflakeBase32Ext>::encode(self)
    }

    fn to_padded_string(&self) -> String {
        self.to_padded_string()
    }
}

impl SnowflakeBitLayout for SnowflakeMastodonId {
    fn id(&self) -> u64 {
        self.id
    }

    fn fields(&self) -> Vec<FieldLayout> {
        vec![
            FieldLayout {
                name: "timestamp",
                bits: 48,
                value: self.timestamp(),
            },
            FieldLayout {
                name: "sequence",
                bits: 16,
                value: self.sequence(),
            },
        ]
    }

    #[cfg(feature = "base32")]
    fn encode(&self) -> String {
        use crate::SnowflakeBase32Ext;
        <Self as SnowflakeBase32Ext>::encode(self)
    }

    fn to_padded_string(&self) -> String {
        self.to_padded_string()
    }
}

impl SnowflakeBitLayout for SnowflakeInstagramId {
    fn id(&self) -> u64 {
        self.id
    }

    fn fields(&self) -> Vec<FieldLayout> {
        vec![
            FieldLayout {
                name: "timestamp",
                bits: 41,
                value: self.timestamp(),
            },
            FieldLayout {
                name: "machine_id",
                bits: 13,
                value: self.machine_id(),
            },
            FieldLayout {
                name: "sequence",
                bits: 10,
                value: self.sequence(),
            },
        ]
    }

    #[cfg(feature = "base32")]
    fn encode(&self) -> String {
        use crate::SnowflakeBase32Ext;
        <Self as SnowflakeBase32Ext>::encode(self)
    }

    fn to_padded_string(&self) -> String {
        self.to_padded_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snowflake_twitter_id_fields_and_bounds() {
        let ts = SnowflakeTwitterId::max_timestamp();
        let mid = SnowflakeTwitterId::max_machine_id();
        let seq = SnowflakeTwitterId::max_sequence();

        let id = SnowflakeTwitterId::from(ts, mid, seq);
        println!("ID: {:?}", id);
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
        println!("ID: {:?}", id);
        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), mid);
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeDiscordId::from_components(ts, mid, seq), id);
    }

    #[test]
    fn test_snowflake_mastodon_id_fields_and_bounds() {
        let ts = SnowflakeMastodonId::max_timestamp();
        let seq = SnowflakeMastodonId::max_sequence();

        let id = SnowflakeMastodonId::from(ts, seq);
        println!("ID: {:?}", id);
        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), 0); // no machine_id
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeMastodonId::from_components(ts, 0, seq), id);
    }

    #[test]
    fn test_snowflake_instagram_id_fields_and_bounds() {
        let ts = SnowflakeInstagramId::max_timestamp();
        let mid = SnowflakeInstagramId::max_machine_id();
        let seq = SnowflakeInstagramId::max_sequence();

        let id = SnowflakeInstagramId::from(ts, mid, seq);
        println!("ID: {:?}", id);
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
}
