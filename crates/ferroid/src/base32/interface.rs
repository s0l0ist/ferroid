use crate::{Base32Error, BeBytes, Error, Id, Result};

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
pub(crate) trait Base32Ext: Id
where
    Self::Ty: BeBytes,
{
    /// Allocates a default, zero-initialized buffer for Base32 encoding.
    ///
    /// This is a convenience method that returns a [`BeBytes::Base32Array`]
    /// suitable for use with [`Base32Ext::enc_to_buf`]. The returned buffer is
    /// stack-allocated, has a fixed size known at compile time, and is
    /// guaranteed to match the Crockford Base32 output size for the backing
    /// integer type.
    ///
    /// See also: [`Base32Ext::enc_to_buf`] for usage.
    ///
    #[inline]
    fn inner_buf() -> <<Self as Id>::Ty as BeBytes>::Base32Array {
        <<Self as Id>::Ty as BeBytes>::Base32Array::default()
    }
    /// Encodes this ID into the provided output buffer without heap allocation.
    ///
    /// This is the zero-allocation alternative to [`Base32Ext::enc`]. The
    /// output buffer must be exactly [`BeBytes::BASE32_SIZE`] bytes in length,
    /// which is guaranteed at compile time when using [`BeBytes::Base32Array`].
    ///
    /// See also: [`Base32Ext::enc`] for an allocation-producing version.
    #[inline]
    fn inner_encode_to_buf(&self, buf: &mut <<Self as Id>::Ty as BeBytes>::Base32Array) {
        super::encode_base32(self.to_raw().to_be_bytes().as_ref(), buf.as_mut());
    }
    /// Decodes a Base32-encoded string back into an ID.
    ///
    /// ⚠️ **Note:** This method performs a structural decode of the Base32
    /// string into the raw underlying integer. It does **not** validate whether
    /// the decoded value satisfies semantic invariants of the ID format (e.g.,
    /// reserved bits).
    ///
    /// If your ID type includes reserved bits, you should explicitly validate
    /// the result using `.is_valid()` or normalize it using `.into_valid()`.
    ///
    /// # Errors
    ///
    /// Returns an error if the input string:
    /// - is not the expected fixed length
    /// - contains invalid ASCII characters (i.e., not in the Crockford Base32
    ///   alphabet)
    #[inline]
    fn inner_decode(s: impl AsRef<str>) -> Result<Self> {
        let s_ref = s.as_ref();
        if s_ref.len() != Self::Ty::BASE32_SIZE {
            return Err(Error::Base32Error(Base32Error::DecodeInvalidLen(
                s_ref.len(),
            )));
        }
        let raw = super::decode_base32(s_ref)?;
        Ok(Self::from_raw(raw))
    }
}

impl<ID> Base32Ext for ID
where
    ID: Id,
    ID::Ty: BeBytes,
{
}
