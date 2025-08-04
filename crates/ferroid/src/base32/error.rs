use crate::Error;
use core::fmt;

/// Errors that can occur while decoding Crockford Base32 strings.
///
/// This error type is generic over the decoded ID type `E`, which allows
/// including the decoded ID in case of overflow. This can help callers inspect
/// or log invalid IDs during error handling.
#[derive(Clone, Debug)]
pub enum Base32Error<E> {
    /// The input string length was invalid.
    ///
    /// Crockford Base32 decodes values in 5-bit chunks. Strings of unexpected
    /// length are rejected to avoid silent truncation or padding.
    DecodeInvalidLen {
        /// The length of the input string.
        len: usize,
    },

    /// The input string contained an invalid ASCII character.
    ///
    /// Only Crockford-compatible characters (0–9, A–Z, excluding I, L, O, U)
    /// are allowed. Characters outside this set (including lowercase or
    /// symbols) will trigger this error.
    DecodeInvalidAscii {
        /// The invalid byte found in the input string.
        byte: u8,
    },

    /// The decoded value exceeds the valid range for the target ID type.
    ///
    /// This usually occurs when the input string sets reserved or unused high
    /// bits. For example, decoding a 13-character string into a Snowflake ID
    /// with reserved upper bits will produce this error, along with the decoded
    /// `id` for inspection.
    DecodeOverflow {
        /// The decoded ID value, which failed validation.
        id: E,
    },
}

impl<E: core::fmt::Debug> fmt::Display for Base32Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecodeInvalidAscii { byte } => write!(f, "invalid ascii byte: {byte}"),
            Self::DecodeInvalidLen { len } => write!(f, "invalid length: {len}"),
            Self::DecodeOverflow { id } => write!(f, "decode overflow: {id:#?}"),
        }
    }
}
impl<E: core::fmt::Debug> core::error::Error for Base32Error<E> {}
impl<E: core::fmt::Debug> From<Base32Error<E>> for Error<E> {
    fn from(err: Base32Error<E>) -> Self {
        Self::Base32Error(err)
    }
}
