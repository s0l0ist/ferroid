use super::interface::Base32Ext;
use crate::{Base32Error, BeBytes, Error, Id, Result, Ulid};
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
pub trait Base32UlidExt: Ulid
where
    Self::Ty: BeBytes,
{
    /// Allocates a default, zero-initialized buffer for Base32 encoding.
    ///
    /// This is a convenience method that returns a [`BeBytes::Base32Array`]
    /// suitable for use with [`Base32UlidExt::encode_to_buf`]. The returned
    /// buffer is stack-allocated, has a fixed size known at compile time, and
    /// is guaranteed to match the Crockford Base32 output size for the backing
    /// integer type.
    ///
    /// See also: [`Base32UlidExt::encode_to_buf`] for usage.
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
    /// #[cfg(all(feature = "base32", feature = "ulid"))]
    /// {   
    ///     use ferroid::{Base32UlidExt, ULID};
    ///     let id = ULID::from_raw(2_424_242_424_242_424_242);
    ///     let encoded = id.encode();
    ///     assert_eq!(encoded, "000000000000023953MG16DJDJ");
    /// }
    /// ```
    fn encode(&self) -> Base32UlidFormatter<Self> {
        Base32UlidFormatter::new(self)
    }
    /// Encodes this ID into the provided output buffer without heap allocation.
    ///
    /// This is the zero-allocation alternative to [`Base32UlidExt::encode`].
    /// The output buffer must be exactly [`BeBytes::BASE32_SIZE`] bytes in
    /// length, which is guaranteed at compile time when using
    /// [`BeBytes::Base32Array`].
    ///
    /// # Example
    ///
    /// ```
    /// #[cfg(all(feature = "base32", feature = "ulid"))]
    /// {   
    ///     use ferroid::{Base32UlidExt, BeBytes, Id, ULID};
    ///     let id = ULID::from_raw(2_424_242_424_242_424_242);
    ///
    ///     // Allocate a zeroed, stack-based buffer with the exact size required for encoding.
    ///     let mut buf = ULID::buf();
    ///     id.encode_to_buf(&mut buf);
    ///
    ///     // SAFETY: Crockford Base32 is guaranteed to produce valid ASCII
    ///     let encoded = unsafe { core::str::from_utf8_unchecked(buf.as_ref()) };
    ///     assert_eq!(encoded, "000000000000023953MG16DJDJ");
    /// }
    /// ```
    ///
    /// See also: [`Base32UlidExt::encode`] for an allocation-producing version.
    fn encode_to_buf<'buf>(
        &self,
        buf: &'buf mut <<Self as Id>::Ty as BeBytes>::Base32Array,
    ) -> Base32UlidFormatterRef<'buf, Self> {
        Base32UlidFormatterRef::new(self, buf)
    }
    /// Decodes a Base32-encoded string back into an ID.
    ///
    /// ⚠️ **Note:**  
    /// This method structurally decodes a Crockford base32 string into an
    /// integer representing a ULID, regardless of whether the input is a
    /// canonical ULID.
    ///
    /// - If the input string's Crockford encoding is larger than the ULID
    ///   spec's maximum (i.e. "7ZZZZZZZZZZZZZZZZZZZZZZZZZ" for 128-bit
    ///   integers), the excess bits are automatically truncated (i.e., the top
    ///   2 bits of the decoded value are discarded), so no overflow or error
    ///   occurs.
    /// - As a result, base32 strings that are technically invalid per the ULID
    ///   spec (i.e., lexicographically greater than the max ULID string) will
    ///   still successfully decode, with the truncated value.
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
    ///     use ferroid::{Base32UlidExt, Ulid, ULID};
    ///     // Crockford base32 encodes in 5-bit chunks, so encoding a 128-bit ULID
    ///     // requires 26 characters (26 * 5 = 130 bits). The two highest (leftmost)
    ///     // bits from base32 encoding are always truncated (ignored) for performance.
    ///     // This means *any* 26-character base32 string decodes structurally to a ULID,
    ///     // regardless of whether it would be considered "out of range" by the ULID spec.
    ///
    ///     // For example, both "7ZZZZZZZZZZZZZZZZZZZZZZZZZ" and "ZZZZZZZZZZZZZZZZZZZZZZZZZZ" are valid:
    ///     // '7' = 0b00111 (top bits 00, rest 111...)
    ///     // 'Z' = 0b11111 (top bits 11, rest 111...)
    ///     assert!(ULID::decode("7ZZZZZZZZZZZZZZZZZZZZZZZZZ").is_ok());
    ///     assert!(ULID::decode("ZZZZZZZZZZZZZZZZZZZZZZZZZZ").is_ok());
    /// }
    /// ```
    fn decode(s: impl AsRef<str>) -> Result<Self> {
        let decoded = Self::inner_decode(s)?;
        if !decoded.is_valid() {
            return Err(Error::Base32Error(Base32Error::DecodeOverflow(
                decoded.to_raw().to_be_bytes().as_ref().to_vec(),
            )));
        }
        Ok(decoded)
    }
}

impl<ID> Base32UlidExt for ID
where
    ID: Ulid,
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
    pub fn as_str(&self) -> &str {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { core::str::from_utf8_unchecked(self.buf.as_ref()) }
    }

    /// Returns an allocated `String` of the base32 encoding.
    pub fn to_string(&self) -> String {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { String::from_utf8_unchecked(self.buf.as_ref().to_vec()) }
    }

    /// Consumes the builder and returns the raw buffer.
    pub fn into_inner(self) -> <T::Ty as BeBytes>::Base32Array {
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

impl<T: Base32UlidExt> PartialEq<String> for Base32UlidFormatter<T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &String) -> bool {
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
    pub fn as_str(&self) -> &str {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { core::str::from_utf8_unchecked(self.buf.as_ref()) }
    }

    /// Returns an allocated `String` of the base32 encoding.
    pub fn to_string(&self) -> String {
        // SAFETY: `self.buf` holds only valid Crockford Base32 ASCII characters
        unsafe { String::from_utf8_unchecked(self.buf.as_ref().to_vec()) }
    }
}

impl<'a, T: Base32UlidExt> fmt::Display for Base32UlidFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'a, T: Base32UlidExt> AsRef<str> for Base32UlidFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<'a, T: Base32UlidExt> PartialEq<str> for Base32UlidFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl<'a, T: Base32UlidExt> PartialEq<&str> for Base32UlidFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl<'a, T: Base32UlidExt> PartialEq<String> for Base32UlidFormatterRef<'a, T>
where
    T::Ty: BeBytes,
{
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other.as_str()
    }
}

#[cfg(all(test, feature = "ulid"))]
mod test {
    use crate::{Base32UlidExt, ULID, Ulid};

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
        let id = ULID::from_components(1469922850259, 1012768647078601740696923);
        assert_eq!(id.timestamp(), 1469922850259);
        assert_eq!(id.random(), 1012768647078601740696923);

        let encoded = id.encode();
        assert_eq!(encoded, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
        let decoded = ULID::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), 1469922850259);
        assert_eq!(decoded.random(), 1012768647078601740696923);
        assert_eq!(id, decoded);

        let id = ULID::from_components(1611559180765, 885339478614498720052741);
        assert_eq!(id.timestamp(), 1611559180765);
        assert_eq!(id.random(), 885339478614498720052741);

        let encoded = id.encode();
        assert_eq!(encoded, "01EWW6K6EXQDX5JV0E9CAHPXG5");
        let decoded = ULID::decode(encoded).unwrap();

        assert_eq!(decoded.timestamp(), 1611559180765);
        assert_eq!(decoded.random(), 885339478614498720052741);
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
}
