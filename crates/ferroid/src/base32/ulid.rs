use super::interface::Base32Ext;
use crate::{Base32Error, BeBytes, Error, Id, Result, UlidId};
use core::fmt;
use core::marker::PhantomData;

/// Extension trait for Crockford Base32 encoding and decoding of ID types.
///
/// This trait enables converting IDs backed by integer types into fixed-length,
/// lexicographically sortable Base32 representation using the [Crockford
/// Base32](https://www.crockford.com/base32.html) alphabet.
pub trait Base32UlidExt: UlidId
where
    Self::Ty: BeBytes,
{
    /// Returns a stack-allocated, zero-initialized buffer for Base32 encoding.
    ///
    /// This is a convenience method that returns a [`BeBytes::Base32Array`]
    /// suitable for use with [`Base32UlidExt::encode_to_buf`]. The returned
    /// buffer is stack-allocated, has a fixed size known at compile time, and
    /// is guaranteed to match the Crockford Base32 output size for the backing
    /// integer type.
    ///
    /// See also: [`Base32UlidExt::encode_to_buf`] for usage.
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
    ///
    /// ```
    /// #[cfg(all(feature = "base32", feature = "ulid"))]
    /// {
    ///     use ferroid::{Base32UlidExt, ULID};
    ///     use std::fmt::Write;
    ///
    ///     let id = ULID::from_raw(2_424_242_424_242_424_242);
    ///
    ///     // Formatter is a view over the internal encoded buffer
    ///     let formatter = id.encode();
    ///
    ///     assert_eq!(formatter, "000000000000023953MG16DJDJ");
    /// }
    /// ```
    fn encode(&self) -> Base32UlidFormatter<Self> {
        Base32UlidFormatter::new(self)
    }
    /// Encodes this ID into the provided buffer without heap allocation and
    /// returns a formatter view over the buffer similar to
    /// [`Base32UlidExt::encode`].
    ///
    /// The buffer must be exactly [`BeBytes::BASE32_SIZE`] bytes long, which is
    /// guaranteed at compile time when using [`Base32UlidExt::buf`].
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(all(feature = "base32", feature = "ulid"))]
    /// {
    ///     use ferroid::{Base32UlidExt, BeBytes, Id, ULID};
    ///
    ///     let id = ULID::from_raw(2_424_242_424_242_424_242);
    ///
    ///     // Stack-allocated buffer of the correct size.
    ///     let mut buf = ULID::buf();
    ///
    ///     // Formatter is a view over the external buffer
    ///     let formatter = id.encode_to_buf(&mut buf);
    ///
    ///     assert_eq!(formatter, "000000000000023953MG16DJDJ");
    ///
    ///     // Or access the raw bytes directly:
    ///     let as_str = unsafe { core::str::from_utf8_unchecked(buf.as_ref()) };
    ///     assert_eq!(as_str, "000000000000023953MG16DJDJ");
    /// }
    /// ```
    ///
    /// See also: [`Base32UlidExt::encode`] for a version that manages its own
    /// buffer.
    fn encode_to_buf<'buf>(
        &self,
        buf: &'buf mut <<Self as Id>::Ty as BeBytes>::Base32Array,
    ) -> Base32UlidFormatterRef<'buf, Self> {
        Base32UlidFormatterRef::new(self, buf)
    }
    /// Decodes a Base32-encoded string back into an ID.
    ///
    /// ⚠️ **Note:**\
    /// This method structurally decodes a Crockford base32 string into an
    /// integer representing a ULID, regardless of whether the input is a
    /// canonical ULID.
    ///
    /// - If the input string's Crockford encoding is larger than the ULID
    ///   spec's maximum (i.e. "7ZZZZZZZZZZZZZZZZZZZZZZZZZ" for 128-bit
    ///   integers), the excess bits are automatically ignored (i.e., the top 2
    ///   bits of the decoded value are discarded), so no overflow or error
    ///   occurs.
    /// - As a result, base32 strings that are technically invalid per the ULID
    ///   spec (i.e., lexicographically greater than the max ULID string) will
    ///   still successfully decode.
    /// - **However**, if your ID type reserves bits (e.g., reserved or unused
    ///   bits in your layout), decoding a string with excess bits may set these
    ///   reserved bits to 1, causing `.is_valid()` to fail, and decode to
    ///   return an error.
    ///
    /// # Errors
    ///
    /// Returns an error if the input string:
    /// - is not the expected fixed length of the backing integer representation
    ///   (i.e. 26 chars for u128)
    /// - contains invalid ASCII characters (i.e., not in the Crockford Base32
    ///   alphabet)
    /// - sets reserved bits that make the decoded value invalid for this ID
    ///   type
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(all(feature = "base32", feature = "ulid"))]
    /// {
    ///    use ferroid::{Base32Error, Base32UlidExt, Error, Id, ULID, UlidId};
    ///
    ///    // Crockford Base32 encodes values in 5-bit chunks, so encoding a u128 (128
    ///    // bits) requires 26 characters (26 × 5 = 130 bits). Since u128 can only hold
    ///    // 128 bits, the highest 2 bits are discarded during decoding.
    ///    //
    ///    // This means *any* 26-character Base32 string will decode into a u128, even
    ///    // if it represents a value that exceeds the canonical range of a specific
    ///    // ID type.
    ///    //
    ///    // Other ID formats may reserve one or more high bits for future use. These
    ///    // reserved bits **must remain unset** for the decoded value to be
    ///    // considered valid.
    ///    //
    ///    // For example, in a `ULID`, "7ZZZZZZZZZZZZZZZZZZZZZZZZZ" represents the
    ///    // largest lexicographically valid encoding that fills all 128 bits with
    ///    // ones. Lexicographically larger values like "ZZZZZZZZZZZZZZZZZZZZZZZZZZ"
    ///    // decode to the *same* ID because their first character differs only in the
    ///    // highest bits (129th & 130th), which are discarded:
    ///    // - '7' = 0b00111 → top bits 00, rest = 111...
    ///    // - 'Z' = 0b11111 → top bits 11, rest = 111...
    ///    //             ↑↑↑ identical after discarding MSBs
    ///    let id1 = ULID::decode("7ZZZZZZZZZZZZZZZZZZZZZZZZZ").unwrap();
    ///    let id2 = ULID::decode("ZZZZZZZZZZZZZZZZZZZZZZZZZZ").unwrap();
    ///    assert_eq!(id1, id2);
    ///
    /// }
    /// ```
    fn decode(s: impl AsRef<str>) -> Result<Self, Error<Self>> {
        let decoded = Self::inner_decode(s)?;
        if !decoded.is_valid() {
            return Err(Error::Base32Error(Base32Error::DecodeOverflow {
                id: decoded,
            }));
        }
        Ok(decoded)
    }
}

impl<ID> Base32UlidExt for ID
where
    ID: UlidId,
    ID::Ty: BeBytes,
{
}

/// A reusable builder that owns the Base32 buffer and formats an ID.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Base32UlidFormatter<T>
where
    T: Base32UlidExt,
    T::Ty: BeBytes,
{
    _id: PhantomData<T>,
    buf: <T::Ty as BeBytes>::Base32Array,
}

impl<T: Base32UlidExt> Base32UlidFormatter<T>
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

impl<T: Base32UlidExt> fmt::Display for Base32UlidFormatter<T>
where
    T::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<T: Base32UlidExt> AsRef<str> for Base32UlidFormatter<T>
where
    T::Ty: BeBytes,
{
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<T: Base32UlidExt> PartialEq<&str> for Base32UlidFormatter<T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(feature = "alloc")]
#[cfg_attr(not(feature = "alloc"), doc(hidden))]
impl<T: Base32UlidExt> PartialEq<alloc::string::String> for Base32UlidFormatter<T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &alloc::string::String) -> bool {
        self.as_str() == other.as_str()
    }
}

/// A builder that borrows a user-supplied buffer for Base32 formatting.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Base32UlidFormatterRef<'a, T>
where
    T: Base32UlidExt,
    T::Ty: BeBytes,
{
    _id: PhantomData<T>,
    buf: &'a <T::Ty as BeBytes>::Base32Array,
}

impl<'a, T: Base32UlidExt> Base32UlidFormatterRef<'a, T>
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

impl<T: Base32UlidExt> fmt::Display for Base32UlidFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<T: Base32UlidExt> AsRef<str> for Base32UlidFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<T: Base32UlidExt> PartialEq<str> for Base32UlidFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl<T: Base32UlidExt> PartialEq<&str> for Base32UlidFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[cfg(feature = "alloc")]
#[cfg_attr(not(feature = "alloc"), doc(hidden))]
impl<T: Base32UlidExt> PartialEq<alloc::string::String> for Base32UlidFormatterRef<'_, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &alloc::string::String) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(all(test, feature = "alloc", feature = "ulid"))]
mod alloc_test {
    use crate::{Base32UlidExt, ULID};
    use alloc::string::ToString;

    #[test]
    fn ulid_display() {
        let ulid = ULID::decode("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        assert_eq!(alloc::format!("{ulid}"), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        assert_eq!(ulid.to_string(), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    }
}

#[cfg(all(test, feature = "ulid"))]
mod test {
    use crate::{Base32Error, Base32UlidExt, Error, UlidId, ULID};

    #[test]
    fn ulid_try_from() {
        let ulid = ULID::try_from("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let encoded = ulid.encode();
        assert_eq!(encoded, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    }

    #[test]
    fn ulid_from_str() {
        use core::str::FromStr;
        let ulid = ULID::from_str("01ARZ3NDEKTSV4RRFFQ69G5FAV").unwrap();
        let encoded = ulid.encode();
        assert_eq!(encoded, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    }

    #[test]
    fn ulid_max() {
        let id = ULID::from_components(ULID::max_timestamp(), ULID::max_random());
        assert_eq!(id.timestamp(), ULID::max_timestamp());
        assert_eq!(id.random(), ULID::max_random());

        let encoded = id.encode();
        assert_eq!(encoded, "7ZZZZZZZZZZZZZZZZZZZZZZZZZ");
        let decoded = ULID::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), ULID::max_timestamp());
        assert_eq!(decoded.random(), ULID::max_random());
        assert_eq!(id, decoded);
    }

    #[test]
    fn ulid_known() {
        let id = ULID::from_components(1_469_922_850_259, 1_012_768_647_078_601_740_696_923);
        assert_eq!(id.timestamp(), 1_469_922_850_259);
        assert_eq!(id.random(), 1_012_768_647_078_601_740_696_923);

        let encoded = id.encode();
        assert_eq!(encoded, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        let decoded = ULID::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), 1_469_922_850_259);
        assert_eq!(decoded.random(), 1_012_768_647_078_601_740_696_923);
        assert_eq!(id, decoded);

        let id = ULID::from_components(1_611_559_180_765, 885_339_478_614_498_720_052_741);
        assert_eq!(id.timestamp(), 1_611_559_180_765);
        assert_eq!(id.random(), 885_339_478_614_498_720_052_741);

        let encoded = id.encode();
        assert_eq!(encoded, "01EWW6K6EXQDX5JV0E9CAHPXG5");
        let decoded = ULID::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), 1_611_559_180_765);
        assert_eq!(decoded.random(), 885_339_478_614_498_720_052_741);
        assert_eq!(id, decoded);
    }

    #[test]
    fn ulid_zero() {
        let id = ULID::from_components(0, 0);
        assert_eq!(id.timestamp(), 0);
        assert_eq!(id.random(), 0);

        let encoded = id.encode();
        assert_eq!(encoded, "00000000000000000000000000");
        let decoded = ULID::decode(&encoded).unwrap();

        assert_eq!(decoded.timestamp(), 0);
        assert_eq!(decoded.random(), 0);
        assert_eq!(id, decoded);
    }

    #[test]
    fn decode_invalid_character_fails() {
        // Base32 Crockford disallows symbols like `@`
        let invalid = "000000000000@0000000000000";
        let res = ULID::decode(invalid);
        assert_eq!(
            res.unwrap_err(),
            Error::Base32Error(Base32Error::DecodeInvalidAscii {
                byte: b'@',
                index: 12,
            })
        );
    }

    #[test]
    fn decode_invalid_length_fails() {
        // Shorter than 26-byte base32 for u128
        let too_short = "0123456789012345678901234";
        let res = ULID::decode(too_short);
        assert_eq!(
            res.unwrap_err(),
            Error::Base32Error(Base32Error::DecodeInvalidLen { len: 25 })
        );

        // Longer than 26-byte base32 for u128
        let too_long = "012345678901234567890123456";
        let res = ULID::decode(too_long);

        assert_eq!(
            res.unwrap_err(),
            Error::Base32Error(Base32Error::DecodeInvalidLen { len: 27 })
        );
    }
}
