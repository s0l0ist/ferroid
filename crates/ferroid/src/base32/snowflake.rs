use super::interface::Base32Ext;
use crate::{Base32Error, BeBytes, Error, Id, Result, Snowflake};
use core::fmt;
use core::marker::PhantomData;

/// Extension trait for types that support Crockford Base32 encoding and
/// decoding.
///
/// This trait enables converting IDs (typically backed by primitive integers)
/// to and from fixed-length, lexicographically sortable Base32 strings using
/// the [Crockford Base32](https://www.crockford.com/base32.html) alphabet.
///
/// It relies on the [`BeBytes`] trait for bit-level access to the underlying
/// integer representation, and produces fixed-width ASCII-encoded output
/// suitable for ordered storage (e.g., in databases, file systems, or URLs).
///
/// # Features
///
/// - Zero-allocation encoding support
/// - Fixed-width, lexicographically sortable output
/// - ASCII-safe encoding using Crockford's Base32 alphabet
/// - Fallible decoding with strong validation
pub trait Base32SnowExt: Snowflake
where
    Self::Ty: BeBytes,
{
    /// Allocates a default, zero-initialized buffer for Base32 encoding.
    ///
    /// This is a convenience method that returns a [`BeBytes::Base32Array`]
    /// suitable for use with [`Base32SnowExt::encode_to_buf`]. The returned
    /// buffer is stack-allocated, has a fixed size known at compile time, and
    /// is guaranteed to match the Crockford Base32 output size for the backing
    /// integer type.
    ///
    /// See also: [`Base32SnowExt::encode_to_buf`] for usage.
    fn buf() -> <<Self as Id>::Ty as BeBytes>::Base32Array {
        <Self as Base32Ext>::inner_buf()
    }
    /// Encodes this ID into a [`String`] using Crockford Base32.
    ///
    /// The resulting string is guaranteed to be ASCII and lexicographically
    /// sortable.
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(all(feature = "base32", feature = "snowflake"))]
    /// {   
    ///     use ferroid::{Base32SnowExt, SnowflakeTwitterId};
    ///     let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///     let encoded = id.encode();
    ///     assert_eq!(encoded, "23953MG16DJDJ");
    /// }
    /// ```
    fn encode(&self) -> Base32SnowFormatter<Self> {
        Base32SnowFormatter::new(self)
    }
    /// Encodes this ID into the provided output buffer without heap allocation.
    ///
    /// This is the zero-allocation alternative to [`Base32SnowExt::encode`].
    /// The output buffer must be exactly [`BeBytes::BASE32_SIZE`] bytes in
    /// length, which is guaranteed at compile time when using
    /// [`BeBytes::Base32Array`].
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(all(feature = "base32", feature = "snowflake"))]
    /// {   
    ///     use ferroid::{Base32SnowExt, BeBytes, Id, SnowflakeTwitterId};
    ///     let id = SnowflakeTwitterId::from_raw(2_424_242_424_242_424_242);
    ///
    ///     // Allocate a zeroed, stack-based buffer with the exact size required for encoding.
    ///     let mut buf = SnowflakeTwitterId::buf();
    ///     id.encode_to_buf(&mut buf);
    ///
    ///     // SAFETY: Crockford Base32 is guaranteed to produce valid ASCII
    ///     let encoded = unsafe { core::str::from_utf8_unchecked(buf.as_ref()) };
    ///     assert_eq!(encoded, "23953MG16DJDJ");
    /// }
    /// ```
    ///
    /// See also: [`Base32SnowExt::encode`] for an allocation-producing version.
    fn encode_to_buf<'buf>(
        &self,
        buf: &'buf mut <<Self as Id>::Ty as BeBytes>::Base32Array,
    ) -> Base32SnowFormatterRef<'buf, Self> {
        Base32SnowFormatterRef::new(self, buf)
    }
    /// Decodes a Base32-encoded string back into an ID.
    ///
    /// ⚠️ **Note:**  
    /// This method structurally decodes a Crockford base32 string into an
    /// integer representing a Snowflake ID, regardless of whether the input is
    /// a canonical Snowflake ID.
    ///
    /// - If the input string's Crockford encoding is larger than the
    ///   Snowflake's maximum (i.e. "FZZZZZZZZZZZZ" for 64-bit integers), the
    ///   excess bit is automatically truncated (i.e., the top 1 bit of the
    ///   decoded value is discarded), so no overflow or error occurs.
    /// - As a result, base32 strings that are technically invalid (i.e.,
    ///   lexicographically greater than the max Snowflake string) will still
    ///   successfully decode, with the truncated value.
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
    ///
    /// ```
    /// #[cfg(all(feature = "base32", feature = "snowflake"))]
    /// {
    ///     use ferroid::{Base32SnowExt, Snowflake, SnowflakeTwitterId, Error, Base32Error, Id};
    ///
    ///     // Crockford base32 encodes in 5-bit chunks, so encoding a u64 (64 bits)
    ///     // requires 13 characters (13 * 5 = 65 bits). The highest (leftmost) bit
    ///     // in the base32 encoding is always truncated (ignored) for performance,
    ///     // so *any* 13-char base32 string decodes to a u64.
    ///
    ///     // Twitter Snowflake IDs reserve the highest bit (the 64th bit).
    ///     // As long as this reserved bit is zero, the decode will succeed.
    ///
    ///     // For example, both "7ZZZZZZZZZZZZ" and "NZZZZZZZZZZZZ" are valid:
    ///     // '7' = 0b00111 (top bit 0, reserved bit 0, rest 111...)
    ///     // 'N' = 0b10111 (top bit 1, reserved bit 0, rest 111...)
    ///     assert!(SnowflakeTwitterId::decode("7ZZZZZZZZZZZZ").is_ok());
    ///     assert!(SnowflakeTwitterId::decode("NZZZZZZZZZZZZ").is_ok());
    ///
    ///     // If the reserved bit is set (e.g., 'F' = 0b01111 or 'Z' = 0b11111), the ID is invalid:
    ///     // 'F' = 0b01111 (top bit X, reserved bit 1, rest 111...).
    ///
    ///     let id = SnowflakeTwitterId::decode("FZZZZZZZZZZZZ").or_else(|err| {
    ///         match err {
    ///             Error::Base32Error(Base32Error::DecodeOverflow(invalid)) => {
    ///                 debug_assert!(!invalid.is_valid());
    ///                 let valid = invalid.into_valid(); // clears reserved bits
    ///                 debug_assert!(valid.is_valid());
    ///                 Ok(valid)
    ///             }
    ///             other => Err(other),
    ///         }
    ///     }).expect("should produce a valid ID");
    /// }
    /// ```
    fn decode(s: impl AsRef<str>) -> Result<Self, Self> {
        let decoded = Self::inner_decode(s)?;
        if !decoded.is_valid() {
            return Err(Error::Base32Error(Base32Error::DecodeOverflow(decoded)));
        }
        Ok(decoded)
    }
}

impl<ID> Base32SnowExt for ID
where
    ID: Snowflake,
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
    pub fn as_str(&self) -> &str {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { core::str::from_utf8_unchecked(self.buf.as_ref()) }
    }

    /// Returns an allocated `String` of the base32 encoding.
    #[cfg(feature = "alloc")]
    #[cfg_attr(not(feature = "alloc"), doc(hidden))]
    #[allow(clippy::inherent_to_string_shadow_display)]
    pub fn to_string(&self) -> alloc::string::String {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { alloc::string::String::from_utf8_unchecked(self.buf.as_ref().to_vec()) }
    }

    /// Consumes the builder and returns the raw buffer.
    pub fn into_inner(self) -> <T::Ty as BeBytes>::Base32Array {
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

#[cfg(feature = "std")]
#[cfg_attr(not(feature = "std"), doc(hidden))]
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
    pub fn as_str(&self) -> &str {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { core::str::from_utf8_unchecked(self.buf.as_ref()) }
    }

    /// Returns an allocated `String` of the base32 encoding.
    #[cfg(feature = "alloc")]
    #[cfg_attr(not(feature = "alloc"), doc(hidden))]
    #[allow(clippy::inherent_to_string_shadow_display)]
    pub fn to_string(&self) -> alloc::string::String {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { alloc::string::String::from_utf8_unchecked(self.buf.as_ref().to_vec()) }
    }
}

impl<'a, T: Base32SnowExt> fmt::Display for Base32SnowFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'a, T: Base32SnowExt> AsRef<str> for Base32SnowFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'a, T: Base32SnowExt> PartialEq<str> for Base32SnowFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl<'a, T: Base32SnowExt> PartialEq<&str> for Base32SnowFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(feature = "alloc")]
#[cfg_attr(not(feature = "alloc"), doc(hidden))]
impl<'a, T: Base32SnowExt> PartialEq<alloc::string::String> for Base32SnowFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &alloc::string::String) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(all(test, feature = "snowflake"))]
mod test {
    use crate::{
        Base32Error, Base32SnowExt, Error, Snowflake, SnowflakeDiscordId, SnowflakeInstagramId,
        SnowflakeMastodonId, SnowflakeTwitterId,
    };

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
        let invalid = "012345678901@";
        let result = SnowflakeTwitterId::decode(invalid);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidAscii(64)))
        ));
    }

    #[test]
    fn decode_invalid_length_fails() {
        // Shorter than 13-byte base32 for u64 (decoded slice won't be 8 bytes)
        let too_short = "012345678901";
        let result = SnowflakeTwitterId::decode(too_short);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidLen(12)))
        ));

        // Longer than 13-byte base32 for u64 (decoded slice won't be 8 bytes)
        let too_long = "01234567890123";
        let result = SnowflakeTwitterId::decode(too_long);
        assert!(matches!(
            result,
            Err(Error::Base32Error(Base32Error::DecodeInvalidLen(14)))
        ));
    }
}
