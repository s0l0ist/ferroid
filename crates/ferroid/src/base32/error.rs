use crate::Error;
use core::fmt;

#[derive(Clone, Debug)]
pub enum Base32Error<E> {
    DecodeInvalidLen(usize),
    DecodeInvalidAscii(u8),
    DecodeOverflow(E),
}
impl<E: core::fmt::Debug> fmt::Display for Base32Error<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DecodeInvalidAscii(b) => write!(f, "invalid ascii byte: {b}"),
            Self::DecodeInvalidLen(len) => write!(f, "invalid length: {len}"),
            Self::DecodeOverflow(bytes) => write!(f, "decode overflow: {bytes:X?}"),
        }
    }
}
impl<E: core::fmt::Debug> core::error::Error for Base32Error<E> {}
impl<E: core::fmt::Debug> From<Base32Error<E>> for Error<E> {
    fn from(err: Base32Error<E>) -> Self {
        Self::Base32Error(err)
    }
}
