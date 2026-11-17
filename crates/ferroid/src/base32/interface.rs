use crate::{
    base32::{Base32Formatter, Base32FormatterRef, Error},
    id::{BeBytes, Id},
};

/// Crockford Base32 encoding and validated decoding for IDs.
///
/// Automatically implemented for every [`Id`] backed by `u32`, `u64`, or `u128`.
///
/// # Example
///
/// ```
/// # #[cfg(feature = "ulid")]
/// # {
/// use ferroid::{base32::Base32Ext, id::ULID};
///
/// let id = ULID::from_raw(42);
/// assert_eq!(ULID::decode(id.encode().as_str()).unwrap(), id);
///
/// let mut buffer = ULID::base32_array();
/// assert_eq!(id.encode_to_buf(&mut buffer).as_str(), id.encode().as_str());
/// # }
/// ```
pub trait Base32Ext: Id
where
    Self::Ty: BeBytes,
{
    /// Returns the backing integer's default byte buffer.
    #[must_use]
    fn byte_array() -> <Self::Ty as BeBytes>::ByteArray {
        <Self::Ty as BeBytes>::ByteArray::default()
    }

    /// Returns the default fixed-size Base32 buffer.
    #[must_use]
    fn base32_array() -> <Self::Ty as BeBytes>::Base32Array {
        <Self::Ty as BeBytes>::Base32Array::default()
    }

    /// Encodes this ID into a formatter with a stack-allocated buffer.
    #[must_use]
    fn encode(&self) -> Base32Formatter<Self> {
        Base32Formatter::new(self)
    }

    /// Encodes into a caller-supplied buffer and returns a formatter view.
    ///
    /// Use [`Self::base32_array`] to create a buffer of the correct size.
    fn encode_to_buf<'buf>(
        &self,
        buffer: &'buf mut <Self::Ty as BeBytes>::Base32Array,
    ) -> Base32FormatterRef<'buf, Self> {
        Base32FormatterRef::new(self, buffer)
    }

    /// Decodes Base32 and validates the resulting ID through [`TryFrom`].
    ///
    /// Bits beyond the backing integer's width are discarded before validation.
    ///
    /// # Errors
    ///
    /// Returns an error for incorrect length, invalid characters, or an invalid
    /// ID layout, such as reserved bits being set.
    fn decode(input: impl AsRef<[u8]>) -> Result<Self, Error<Self>>
    where
        Self: TryFrom<<Self as Id>::Ty, Error = Error<Self>>,
    {
        let bytes = input.as_ref();
        if bytes.len() != Self::Ty::BASE32_SIZE {
            return Err(Error::DecodeInvalidLen { len: bytes.len() });
        }
        Self::try_from(super::decode_base32(bytes)?)
    }
}

impl<ID> Base32Ext for ID
where
    ID: Id,
    ID::Ty: BeBytes,
{
}
