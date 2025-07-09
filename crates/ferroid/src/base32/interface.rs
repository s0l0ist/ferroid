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
pub trait Base32Ext: Id
where
    Self::Ty: BeBytes,
{
    /// Encodes this ID into a [`String`] using Crockford Base32.
    ///
    /// The resulting string is guaranteed to be ASCII and lexicographically
    /// sortable.
    fn enc(&self) -> String {
        let mut buf = <Self::Ty as BeBytes>::Base32Array::default();
        self.enc_to_buf(&mut buf);

        // SAFETY: Crockford Base32 output is always valid ASCII
        unsafe { String::from_utf8_unchecked(buf.as_ref().to_vec()) }
    }
    /// Encodes this ID into the provided output buffer without heap allocation.
    ///
    /// This is the zero-allocation alternative to [`Base32Ext::enc`]. The
    /// output buffer must be exactly [`BeBytes::BASE32_SIZE`] bytes in length,
    /// which is guaranteed at compile time when using [`BeBytes::Base32Array`].
    ///
    /// See also: [`Base32Ext::enc`] for an allocation-producing version.
    fn enc_to_buf(&self, buf: &mut <<Self as Id>::Ty as BeBytes>::Base32Array) {
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
    fn dec(s: &str) -> Result<Self> {
        if s.len() != Self::Ty::BASE32_SIZE {
            return Err(Error::Base32Error(Base32Error::DecodeInvalidLen(s.len())));
        }
        let raw = super::decode_base32(s)?;
        Ok(Self::from_raw(raw))
    }
}

impl<ID> Base32Ext for ID
where
    ID: Id,
    ID::Ty: BeBytes,
{
}
