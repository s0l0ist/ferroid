use core::fmt;

/// Errors that can occur while decoding Crockford Base32 strings.
///
/// This error type is generic over the decoded ID type `E`, which allows
/// including the decoded ID in case of overflow. This can help callers inspect
/// or log invalid IDs during error handling.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Base32Error<E> {
    /// The input string length was invalid.
    ///
    /// Crockford Base32 decodes values in 5-bit chunks. Strings of unexpected
    /// length are rejected to avoid silent truncation or padding.
    DecodeInvalidLen {
        /// The length of the input string.
        len: usize,
    },

    /// The input contained a character that is not valid Crockford Base32.
    ///
    /// Accepts digits `0–9`, uppercase letters `A–Z`, and lowercase letters.
    /// Aliases like `O/o → 0` and `I/i/L/l → 1` are also supported.
    ///
    /// This error is returned when a non-alphanumeric or non-ASCII character
    /// (e.g. `@`, `*`, `~`) is encountered during decoding.
    DecodeInvalidAscii {
        /// The invalid byte found in the input string.
        byte: u8,
        /// The index of the invalid byte in the input string.
        index: usize,
    },

    /// The decoded value exceeds the valid range for the target ID type.
    ///
    /// This occurs when the input string sets reserved or unused high bits.
    DecodeOverflow {
        /// The decoded ID value, which failed validation.
        id: E,
    },
}

impl<E: core::fmt::Debug> fmt::Display for Base32Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecodeInvalidAscii { byte, index } => {
                write!(f, "invalid ascii byte ({byte}) at index ({index})")
            }
            Self::DecodeInvalidLen { len } => write!(f, "invalid length: {len}"),
            Self::DecodeOverflow { id } => write!(f, "decode overflow: {id:#?}"),
        }
    }
}
impl<E: core::fmt::Debug> core::error::Error for Base32Error<E> {}
