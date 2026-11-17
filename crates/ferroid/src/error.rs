use core::fmt;

/// Errors that can occur while converting integers or Base32 strings into IDs.
///
/// This error type is generic over the decoded ID type `ID`, which allows
/// including the decoded ID in case of overflow. This can help callers inspect
/// or log invalid IDs during error handling.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Error<ID> {
    /// The converted value violates the target ID's layout invariants.
    ///
    /// For macro-generated IDs, this means reserved bits are set.
    DecodeOverflow {
        /// The decoded ID value, which failed validation.
        id: ID,
    },

    /// The input string length was invalid.
    ///
    /// Crockford Base32 decodes values in 5-bit chunks. Strings of unexpected
    /// length are rejected to avoid silent truncation or padding.
    #[cfg(feature = "base32")]
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
    #[cfg(feature = "base32")]
    DecodeInvalidAscii {
        /// The invalid byte found in the input string.
        byte: u8,
        /// The index of the invalid byte in the input string.
        index: usize,
    },
}

impl<ID: fmt::Debug> fmt::Display for Error<ID> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(feature = "base32")]
            Self::DecodeInvalidAscii { byte, index } => {
                write!(f, "invalid ascii byte ({byte}) at index ({index})")
            }
            #[cfg(feature = "base32")]
            Self::DecodeInvalidLen { len } => write!(f, "invalid length: {len}"),
            Self::DecodeOverflow { id } => write!(f, "decode overflow: {id:#?}"),
        }
    }
}
impl<ID: fmt::Debug> core::error::Error for Error<ID> {}
