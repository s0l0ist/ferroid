use core::fmt;
use std::{
    hash::Hash,
    ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Sub, SubAssign},
};

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
    type Ty: Ord
        + Copy
        + Add<Output = Self::Ty>
        + AddAssign
        + Sub<Output = Self::Ty>
        + SubAssign
        + Mul<Output = Self::Ty>
        + MulAssign
        + Div<Output = Self::Ty>
        + DivAssign
        + Into<Self::Ty>
        + Into<u64>
        + From<Self::Ty>
        + From<u8>
        + From<u16>
        + From<u32>
        + From<u64>
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

/// A 64-bit Snowflake ID using the classic Twitter layout
///
/// - 41 bits timestamp
/// - 10 bits machine ID
/// - 12 bits sequence
///
/// ```text
/// | timestamp (41) | machine_id (10) | sequence (12) |
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnowflakeTwitterId {
    id: u64,
}

impl SnowflakeTwitterId {
    /// Bitmask for extracting the 41-bit timestamp field from a packed ID.
    /// Stored in bits 22 through 62 (excluding the sign bit at bit 63).
    pub const TIMESTAMP_MASK: u64 = (1 << 41) - 1;

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

    /// Number of bits to shift the sequence field (always 0, since it starts at
    /// the LSB).
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

/// A 64-bit Snowflake ID using the Discord layout
///
/// NOTE: To keep the API the same, we're collapsing their notation of machine
/// and process id into the same number of bits.
///
/// - 42 bits timestamp (ms since Discord epoch: Jan 1, 2015 UTC)
/// - 10 bits internal worker ID (5) and process ID (5), combined into
///   machine_id (10)
/// - 12 bits sequence
///
/// ```text
/// | timestamp (42) | machine_id (10) | sequence (12) |
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnowflakeDiscordId {
    id: u64,
}

impl SnowflakeDiscordId {
    /// Bitmask for extracting the 42-bit timestamp field from a packed ID.
    /// Occupies bits 22 through 63 (most significant bits).
    pub const TIMESTAMP_MASK: u64 = (1 << 42) - 1;

    /// Bitmask for extracting the 10-bit machine ID field. Occupies bits 12
    /// through 21.
    pub const MACHINE_ID_MASK: u64 = (1 << 10) - 1;

    /// Bitmask for extracting the 12-bit sequence field. Occupies bits 0
    /// through 11.
    pub const SEQUENCE_MASK: u64 = (1 << 12) - 1;

    /// Number of bits to shift the timestamp to its correct position (starting
    /// at bit 22).
    pub const TIMESTAMP_SHIFT: u64 = 22;

    /// Number of bits to shift the process/machine ID to its correct position.
    pub const MACHINE_ID_SHIFT: u64 = 12;

    /// Number of bits to shift the sequence field (starts at the least
    /// significant bit).
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

/// A 64-bit Snowflake ID using the Mastodon layout
///
/// - 48 bits timestamp (ms since UNIX epoch)
/// - 16 bits sequence
///
/// ```text
/// | timestamp (48) | sequence (16) |
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnowflakeMastodonId {
    id: u64,
}

impl SnowflakeMastodonId {
    /// Mask for 48-bit timestamp field (used to extract the upper 48 bits).
    /// This corresponds to millisecond precision since the UNIX epoch.
    pub const TIMESTAMP_MASK: u64 = (1 << 48) - 1;

    /// Mask for 16-bit sequence field (used to extract the lower 16 bits). This
    /// provides up to 65,536 unique IDs per millisecond.
    pub const SEQUENCE_MASK: u64 = (1 << 16) - 1;

    /// Bit offset for timestamp (starts at bit 16, leaving room for 16-bit
    /// sequence).
    pub const TIMESTAMP_SHIFT: u64 = 16;

    /// Bit offset for sequence (least significant 16 bits).
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

/// A 64-bit Snowflake ID using the Instagram layout
///
/// - 41 bits timestamp (ms since UNIX epoch)
/// - 13 bits machine ID
/// - 10 bits sequence
///
/// ```text
/// | timestamp (48) | machine(13) | sequence (10) |
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SnowflakeInstagramId {
    id: u64,
}

impl SnowflakeInstagramId {
    /// Mask for 41-bit timestamp field (milliseconds since a custom epoch).
    /// Ensures timestamps stay within the expected range and fit in the upper
    /// bits.
    pub const TIMESTAMP_MASK: u64 = (1 << 41) - 1;

    /// Mask for 13-bit machine ID field (shard or datacenter). Supports up to
    /// 8192 unique machine IDs.
    pub const MACHINE_ID_MASK: u64 = (1 << 13) - 1;

    /// Mask for 10-bit sequence field. Allows up to 1024 unique IDs to be
    /// generated per millisecond per machine.
    pub const SEQUENCE_MASK: u64 = (1 << 10) - 1;

    /// Bit offset for timestamp (starts at bit 23).
    pub const TIMESTAMP_SHIFT: u64 = 23;

    /// Bit offset for machine ID (starts at bit 10).
    pub const MACHINE_ID_SHIFT: u64 = 10;

    /// Bit offset for sequence (least significant 10 bits).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snowflake_instagram_id_fields_and_bounds() {
        let ts = SnowflakeInstagramId::max_timestamp();
        let mid = SnowflakeInstagramId::max_machine_id();
        let seq = SnowflakeInstagramId::max_sequence();

        let id = SnowflakeInstagramId::from(ts, mid, seq);

        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), mid);
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeInstagramId::from_components(ts, mid, seq), id);
    }

    #[test]
    fn test_snowflake_mastodon_id_fields_and_bounds() {
        let ts = SnowflakeMastodonId::max_timestamp();
        let seq = SnowflakeMastodonId::max_sequence();

        let id = SnowflakeMastodonId::from(ts, seq);

        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), 0); // no machine_id
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeMastodonId::from_components(ts, 0, seq), id);
    }

    #[test]
    fn test_snowflake_twitter_id_fields_and_bounds() {
        let ts = SnowflakeTwitterId::max_timestamp();
        let mid = SnowflakeTwitterId::max_machine_id();
        let seq = SnowflakeTwitterId::max_sequence();

        let id = SnowflakeTwitterId::from(ts, mid, seq);

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

        assert_eq!(id.timestamp(), ts);
        assert_eq!(id.machine_id(), mid);
        assert_eq!(id.sequence(), seq);
        assert_eq!(SnowflakeDiscordId::from_components(ts, mid, seq), id);
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
}
