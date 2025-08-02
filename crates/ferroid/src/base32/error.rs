use crate::Error;
use core::fmt;

#[derive(Clone, Debug)]
pub enum Base32Error<E> {
    DecodeInvalidLen { len: usize },
    DecodeInvalidAscii { byte: u8 },
    DecodeOverflow { id: E },
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
