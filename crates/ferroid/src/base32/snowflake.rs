use super::interface::Base32Ext;
use crate::{Base32Error, BeBytes, Id, Result, SnowflakeId};
use core::fmt;
use core::marker::PhantomData;

/// Extension trait for Crockford Base32 encoding and decoding of ID types.
///
/// This trait enables converting IDs backed by integer types into fixed-length,
/// lexicographically sortable Base32 representation using the [Crockford
/// Base32](https://www.crockford.com/base32.html) alphabet.
pub trait Base32SnowExt: SnowflakeId
where
    Self::Ty: BeBytes,
{
    /// Returns a stack-allocated, zero-initialized buffer for Base32 encoding.
    ///
    /// This is a convenience method that returns a [`BeBytes::Base32Array`]
    /// suitable for use with [`Base32SnowExt::encode_to_buf`]. The returned
    /// buffer is stack-allocated, has a fixed size known at compile time, and
    /// is guaranteed to match the Crockford Base32 output size for the backing
    /// integer type.
    ///
    /// See also: [`Base32SnowExt::encode_to_buf`] for usage.
    #[must_use]
    fn buf() -> <<Self as Id>::Ty as BeBytes>::Base32Array {
        <Self as Base32Ext>::inner_buf()
    }
    /// Returns a formatter containing the Crockford Base32 representation of
    /// the ID.
    ///
    /// The formatter is a lightweight, zero-allocation view over that internal
    /// buffer that implements [`core::fmt::Display`] and [`AsRef<str>`].
    ///
    /// # Example
    /// ```
    /// use ferroid::{Base32SnowExt, SnowflakeTwitterId};
    /// use std::fmt::Write;
    ///
    /// let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///
    /// // Formatter is a view over the internal encoded buffer
    /// let formatter = id.encode();
    ///
    /// assert_eq!(formatter, "23953MG16DJDJ");
    /// ```
    fn encode(&self) -> Base32SnowFormatter<Self> {
        Base32SnowFormatter::new(self)
    }
    /// Encodes this ID into the provided buffer without heap allocation and
    /// returns a formatter view over the buffer similar to
    /// [`Base32SnowExt::encode`].
    ///
    /// The buffer must be exactly [`BeBytes::BASE32_SIZE`] bytes long, which is
    /// guaranteed at compile time when using [`Base32SnowExt::buf`].
    /// # Example
    /// ```
    /// use ferroid::{Base32SnowExt, BeBytes, Id, SnowflakeTwitterId};
    ///
    /// let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///
    /// // Stack-allocated buffer of the correct size.
    /// let mut buf = SnowflakeTwitterId::buf();
    ///
    /// // Formatter is a view over the external buffer
    /// let formatter = id.encode_to_buf(&mut buf);
    ///
    /// assert_eq!(formatter, "23953MG16DJDJ");
    ///
    /// // Or access the raw bytes directly:
    /// let as_str = unsafe { core::str::from_utf8_unchecked(buf.as_ref()) };
    /// assert_eq!(as_str, "23953MG16DJDJ");
    /// ```
    ///
    /// See also: [`Base32SnowExt::encode`] for a version that manages its own
    /// buffer.
    fn encode_to_buf<'buf>(
        &self,
        buf: &'buf mut <<Self as Id>::Ty as BeBytes>::Base32Array,
    ) -> Base32SnowFormatterRef<'buf, Self> {
        Base32SnowFormatterRef::new(self, buf)
    }
    /// Decodes a Base32-encoded string back into an ID.
    ///
    /// ⚠️ **Note:**\
    /// This method structurally decodes a Crockford base32 string into an
    /// integer representing a Snowflake ID, regardless of whether the input is
    /// a canonical Snowflake ID.
    ///
    /// - If the input string's Crockford encoding is larger than the
    ///   Snowflake's maximum (i.e. "FZZZZZZZZZZZZ" for 64-bit integers), the
    ///   excess bit is automatically ignored (i.e., the top 1 bit of the
    ///   decoded value is discarded), so no overflow or error occurs.
    /// - As a result, base32 strings that are technically invalid (i.e.,
    ///   lexicographically greater than the max Snowflake string) will still
    ///   successfully decode.
    /// - **However**, if your ID type reserves bits (e.g., reserved or unused
    ///   bits in your layout), decoding a string with excess bits may set these
    ///   reserved bits to 1, causing `.is_valid()` to fail, and decode to
    ///   return an error.
    ///
    /// # Errors
    ///
    /// Returns an error if the input string:
    /// - is not the expected fixed length of the backing integer representation
    ///   (i.e. 13 chars for u64, 26 chars for u128)
    /// - contains invalid ASCII characters (i.e., not in the Crockford Base32
    ///   alphabet)
    /// - sets reserved bits that make the decoded value invalid for this ID
    ///   type
    ///
    /// # Example
    /// ```
    /// use ferroid::{Base32Error, Base32SnowExt, Id, SnowflakeId, SnowflakeTwitterId};
    ///
    /// // Crockford Base32 encodes values in 5-bit chunks, so encoding a u64
    /// // (64 bits)
    /// // requires 13 characters (13 × 5 = 65 bits). Since u64 can only hold 64
    /// // bits, the highest (leftmost) bit is discarded during decoding.
    /// //
    /// // This means *any* 13-character Base32 string will decode into a u64, even
    /// // if it represents a value that exceeds the canonical range of a specific
    /// // ID type.
    /// //
    /// // Many ID formats (such as Twitter Snowflake IDs) reserve one or more high
    /// // bits for future use. These reserved bits **must remain unset** for the
    /// // decoded value to be considered valid.
    /// //
    /// // For example, in a `SnowflakeTwitterId`, "7ZZZZZZZZZZZZ" represents the
    /// // largest lexicographically valid encoding that fills all non-reserved bits
    /// // with ones. Lexicographically larger values like "QZZZZZZZZZZZZ" decode to
    /// // the *same* ID because their first character differs only in the highest
    /// // (65th) bit, which is discarded:
    /// // - '7' = 0b00111 → top bit 0, reserved bit 0, rest = 111...
    /// // - 'Q' = 0b10111 → top bit 1, reserved bit 0, rest = 111...
    /// //            ↑↑↑↑ identical after discarding MSB
    /// let id1 = SnowflakeTwitterId::decode("7ZZZZZZZZZZZZ").unwrap();
    /// let id2 = SnowflakeTwitterId::decode("QZZZZZZZZZZZZ").unwrap();
    /// assert_eq!(id1, id2);
    ///
    /// // In contrast, "PZZZZZZZZZZZZ" differs in more significant bits and decodes
    /// // to a distinct value:
    /// // - 'P' = 0b10110 → top bit 1, reserved bit 0, rest = 110...
    /// //               ↑ alters bits within the ID layout beyond the reserved region
    /// let id3 = SnowflakeTwitterId::decode("PZZZZZZZZZZZZ").unwrap();
    /// assert_ne!(id1, id3);
    ///
    /// // If the reserved bits are set (e.g., 'F' = 0b01111 or 'Z' = 0b11111),
    /// // decoding fails and the invalid ID is returned:
    /// // - 'F' = 0b01111 → top bit 0, reserved bit 1, rest = 111...
    /// //            ↑ reserved bit is set - ID is invalid
    /// let id = SnowflakeTwitterId::decode("FZZZZZZZZZZZZ")
    ///     .or_else(|err| {
    ///         match err {
    ///             Base32Error::DecodeOverflow { id } => {
    ///                 debug_assert!(!id.is_valid());
    ///                 // clears reserved bits
    ///                 let valid = id.into_valid();
    ///                 debug_assert!(valid.is_valid());
    ///                 Ok(valid)
    ///             }
    ///             other => Err(other),
    ///         }
    ///     })
    ///     .expect("should produce a valid ID");
    /// ```
    fn decode(s: impl AsRef<str>) -> Result<Self, Base32Error<Self>> {
        let decoded = Self::inner_decode(s)?;
        if !decoded.is_valid() {
            return Err(Base32Error::DecodeOverflow { id: decoded });
        }
        Ok(decoded)
    }
}

impl<ID> Base32SnowExt for ID
where
    ID: SnowflakeId,
    ID::Ty: BeBytes,
{
}

/// A reusable builder that owns the Base32 buffer and formats an ID.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Base32SnowFormatter<T>
where
    T: Base32SnowExt,
    T::Ty: BeBytes,
{
    _id: PhantomData<T>,
    buf: <T::Ty as BeBytes>::Base32Array,
}

impl<T: Base32SnowExt> Base32SnowFormatter<T>
where
    T::Ty: BeBytes,
{
    pub fn new(id: &T) -> Self {
        let mut buf = T::buf();
        id.inner_encode_to_buf(&mut buf);
        Self {
            _id: PhantomData,
            buf,
        }
    }

    /// Returns a `&str` view of the base32 encoding.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { core::str::from_utf8_unchecked(self.buf.as_ref()) }
    }

    /// Returns an allocated `String` of the base32 encoding.
    #[cfg(feature = "alloc")]
    #[cfg_attr(not(feature = "alloc"), doc(hidden))]
    #[allow(clippy::inherent_to_string_shadow_display)]
    #[must_use]
    pub fn to_string(&self) -> alloc::string::String {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { alloc::string::String::from_utf8_unchecked(self.buf.as_ref().to_vec()) }
    }

    /// Consumes the builder and returns the raw buffer.
    pub const fn into_inner(self) -> <T::Ty as BeBytes>::Base32Array {
        self.buf
    }
}

impl<T: Base32SnowExt> fmt::Display for Base32SnowFormatter<T>
where
    T::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<T: Base32SnowExt> AsRef<str> for Base32SnowFormatter<T>
where
    T::Ty: BeBytes,
{
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<T: Base32SnowExt> PartialEq<&str> for Base32SnowFormatter<T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(feature = "alloc")]
#[cfg_attr(not(feature = "alloc"), doc(hidden))]
impl<T: Base32SnowExt> PartialEq<alloc::string::String> for Base32SnowFormatter<T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &alloc::string::String) -> bool {
        self.as_str() == other.as_str()
    }
}

/// A builder that borrows a user-supplied buffer for Base32 formatting.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Base32SnowFormatterRef<'a, T>
where
    T: Base32SnowExt,
    T::Ty: BeBytes,
{
    _id: PhantomData<T>,
    buf: &'a <T::Ty as BeBytes>::Base32Array,
}

impl<'a, T: Base32SnowExt> Base32SnowFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    pub fn new(id: &T, buf: &'a mut <T::Ty as BeBytes>::Base32Array) -> Self {
        id.inner_encode_to_buf(buf);
        Self {
            _id: PhantomData,
            buf,
        }
    }

    /// Returns a `&str` view of the base32 encoding.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { core::str::from_utf8_unchecked(self.buf.as_ref()) }
    }

    /// Returns an allocated `String` of the base32 encoding.
    #[cfg(feature = "alloc")]
    #[cfg_attr(not(feature = "alloc"), doc(hidden))]
    #[allow(clippy::inherent_to_string_shadow_display)]
    #[must_use]
    pub fn to_string(&self) -> alloc::string::String {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { alloc::string::String::from_utf8_unchecked(self.buf.as_ref().to_vec()) }
    }
}

impl<T: Base32SnowExt> fmt::Display for Base32SnowFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<T: Base32SnowExt> AsRef<str> for Base32SnowFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<T: Base32SnowExt> PartialEq<str> for Base32SnowFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl<T: Base32SnowExt> PartialEq<&str> for Base32SnowFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(feature = "alloc")]
#[cfg_attr(not(feature = "alloc"), doc(hidden))]
impl<T: Base32SnowExt> PartialEq<alloc::string::String> for Base32SnowFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &alloc::string::String) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(all(test, feature = "alloc", feature = "snowflake"))]
mod alloc_test {
    use crate::{
        Base32SnowExt, SnowflakeDiscordId, SnowflakeInstagramId, SnowflakeMastodonId,
        SnowflakeTwitterId,
    };
    use alloc::string::ToString;

    #[test]
    fn twitter_display() {
        let id = SnowflakeTwitterId::decode("01ARZ3NDEKTSV").unwrap();
        assert_eq!(alloc::format!("{id}"), "01ARZ3NDEKTSV");
        assert_eq!(id.to_string(), "01ARZ3NDEKTSV");
    }
    #[test]
    fn instagram_display() {
        let id = SnowflakeInstagramId::decode("01ARZ3NDEKTSV").unwrap();
        assert_eq!(alloc::format!("{id}"), "01ARZ3NDEKTSV");
        assert_eq!(id.to_string(), "01ARZ3NDEKTSV");
    }
    #[test]
    fn mastodon_display() {
        let id = SnowflakeMastodonId::decode("01ARZ3NDEKTSV").unwrap();
        assert_eq!(alloc::format!("{id}"), "01ARZ3NDEKTSV");
        assert_eq!(id.to_string(), "01ARZ3NDEKTSV");
    }
    #[test]
    fn discord_display() {
        let id = SnowflakeDiscordId::decode("01ARZ3NDEKTSV").unwrap();
        assert_eq!(alloc::format!("{id}"), "01ARZ3NDEKTSV");
        assert_eq!(id.to_string(), "01ARZ3NDEKTSV");
    }
}

#[cfg(all(test, feature = "snowflake"))]
mod test {
    use crate::{
        Base32Error, Base32SnowExt, SnowflakeDiscordId, SnowflakeId, SnowflakeInstagramId,
        SnowflakeMastodonId, SnowflakeTwitterId,
    };

    #[test]
    fn snow_try_from() {
        // Don't need to test all IDs
        let id = SnowflakeTwitterId::try_from("01ARZ3NDEKTSV").unwrap();
        let encoded = id.encode();
        assert_eq!(encoded, "01ARZ3NDEKTSV");
    }

    #[test]
    fn snow_from_str() {
        // Don't need to test all IDs
        use core::str::FromStr;
        let id = SnowflakeTwitterId::from_str("01ARZ3NDEKTSV").unwrap();
        let encoded = id.encode();
        assert_eq!(encoded, "01ARZ3NDEKTSV");
    }

    #[test]
    fn twitter_max() {
        let id = SnowflakeTwitterId::from_components(
            SnowflakeTwitterId::max_timestamp(),
            SnowflakeTwitterId::max_machine_id(),
            SnowflakeTwitterId::max_sequence(),
        );
        assert_eq!(id.timestamp(), SnowflakeTwitterId::max_timestamp());
        assert_eq!(id.machine_id(), SnowflakeTwitterId::max_machine_id());
        assert_eq!(id.sequence(), SnowflakeTwitterId::max_sequence());

        let encoded = id.encode();
        assert_eq!(encoded, "7ZZZZZZZZZZZZ");
        let decoded = SnowflakeTwitterId::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), SnowflakeTwitterId::max_timestamp());
        assert_eq!(decoded.machine_id(), SnowflakeTwitterId::max_machine_id());
        assert_eq!(decoded.sequence(), SnowflakeTwitterId::max_sequence());
        assert_eq!(id, decoded);
    }

    #[test]
    fn twitter_zero() {
        let id = SnowflakeTwitterId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "0000000000000");
        let decoded = SnowflakeTwitterId::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.machine_id(), 0);
        assert_eq!(decoded.sequence(), 0);
        assert_eq!(id, decoded);
    }

    #[test]
    fn discord_max() {
        let id = SnowflakeDiscordId::from_components(
            SnowflakeDiscordId::max_timestamp(),
            SnowflakeDiscordId::max_machine_id(),
            SnowflakeDiscordId::max_sequence(),
        );
        assert_eq!(id.timestamp(), SnowflakeDiscordId::max_timestamp());
        assert_eq!(id.machine_id(), SnowflakeDiscordId::max_machine_id());
        assert_eq!(id.sequence(), SnowflakeDiscordId::max_sequence());

        let encoded = id.encode();
        assert_eq!(encoded, "FZZZZZZZZZZZZ");
        let decoded = SnowflakeDiscordId::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), SnowflakeDiscordId::max_timestamp());
        assert_eq!(decoded.machine_id(), SnowflakeDiscordId::max_machine_id());
        assert_eq!(decoded.sequence(), SnowflakeDiscordId::max_sequence());
        assert_eq!(id, decoded);
    }

    #[test]
    fn discord_zero() {
        let id = SnowflakeDiscordId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "0000000000000");
        let decoded = SnowflakeDiscordId::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.machine_id(), 0);
        assert_eq!(decoded.sequence(), 0);
        assert_eq!(id, decoded);
    }

    #[test]
    fn instagram_max() {
        let id = SnowflakeInstagramId::from_components(
            SnowflakeInstagramId::max_timestamp(),
            SnowflakeInstagramId::max_machine_id(),
            SnowflakeInstagramId::max_sequence(),
        );
        assert_eq!(id.timestamp(), SnowflakeInstagramId::max_timestamp());
        assert_eq!(id.machine_id(), SnowflakeInstagramId::max_machine_id());
        assert_eq!(id.sequence(), SnowflakeInstagramId::max_sequence());

        let encoded = id.encode();
        assert_eq!(encoded, "FZZZZZZZZZZZZ");
        let decoded = SnowflakeInstagramId::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), SnowflakeInstagramId::max_timestamp());
        assert_eq!(decoded.machine_id(), SnowflakeInstagramId::max_machine_id());
        assert_eq!(decoded.sequence(), SnowflakeInstagramId::max_sequence());
        assert_eq!(id, decoded);
    }

    #[test]
    fn instagram_zero() {
        let id = SnowflakeInstagramId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "0000000000000");
        let decoded = SnowflakeInstagramId::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.machine_id(), 0);
        assert_eq!(decoded.sequence(), 0);
        assert_eq!(id, decoded);
    }

    #[test]
    fn mastodon_max() {
        let id = SnowflakeMastodonId::from_components(
            SnowflakeMastodonId::max_timestamp(),
            SnowflakeMastodonId::max_machine_id(),
            SnowflakeMastodonId::max_sequence(),
        );
        assert_eq!(id.timestamp(), SnowflakeMastodonId::max_timestamp());
        assert_eq!(id.machine_id(), SnowflakeMastodonId::max_machine_id());
        assert_eq!(id.sequence(), SnowflakeMastodonId::max_sequence());

        let encoded = id.encode();
        assert_eq!(encoded, "FZZZZZZZZZZZZ");
        let decoded = SnowflakeMastodonId::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), SnowflakeMastodonId::max_timestamp());
        assert_eq!(decoded.machine_id(), SnowflakeMastodonId::max_machine_id());
        assert_eq!(decoded.sequence(), SnowflakeMastodonId::max_sequence());
        assert_eq!(id, decoded);
    }

    #[test]
    fn mastodon_zero() {
        let id = SnowflakeMastodonId::from_components(0, 0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.machine_id(), 0);
        assert_eq!(id.sequence(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "0000000000000");
        let decoded = SnowflakeMastodonId::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.machine_id(), 0);
        assert_eq!(decoded.sequence(), 0);
        assert_eq!(id, decoded);
    }

    #[test]
    fn decode_invalid_character_fails() {
        // Base32 Crockford disallows symbols like `@`
        let invalid = "000000@000000";
        let res = SnowflakeTwitterId::decode(invalid);
        assert_eq!(
            res.unwrap_err(),
            Base32Error::DecodeInvalidAscii {
                byte: b'@',
                index: 6,
            }
        );
    }

    #[test]
    fn decode_invalid_length_fails() {
        // Shorter than 13-byte base32 for u64
        let too_short = "012345678901";
        let res = SnowflakeTwitterId::decode(too_short);
        assert_eq!(res.unwrap_err(), Base32Error::DecodeInvalidLen { len: 12 });

        // Longer than 13-byte base32 for u64
        let too_long = "01234567890123";
        let res = SnowflakeTwitterId::decode(too_long);

        assert_eq!(res.unwrap_err(), Base32Error::DecodeInvalidLen { len: 14 });
    }
}
