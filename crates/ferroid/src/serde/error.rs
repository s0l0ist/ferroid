use crate::Error;
use core::fmt;

/// Errors that can occur while decoding native integers.
///
/// This error type is generic over the decoded ID type `E`, which allows
/// including the decoded ID in case of overflow. This can help callers inspect
/// or log invalid IDs during error handling.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SerdeError<E> {
    /// The decoded value exceeds the valid range for the target ID type.
    ///
    /// This occurs when the input string sets reserved or unused high bits.
    DecodeOverflow {
        /// The decoded ID value, which failed validation.
        id: E,
    },

    /// An error occurred during Crockford Base32 decoding.
    ///
    /// This wraps the [`crate::Base32Error`] type and is only available when
    /// the `base32` feature is enabled.
    #[cfg_attr(docsrs, doc(cfg(feature = "base32")))]
    #[cfg(feature = "base32")]
    Base32Error(crate::Base32Error<E>),
}

impl<E: fmt::Debug> fmt::Display for SerdeError<E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{self:?}")
    }
}
impl<E: core::fmt::Debug> core::error::Error for SerdeError<E> {}
impl<E: core::fmt::Debug> From<SerdeError<E>> for Error<E> {
    fn from(err: SerdeError<E>) -> Self {
        Self::SerdeError(err)
    }
}
