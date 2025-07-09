use crate::Error;
use core::fmt;

#[derive(Clone, Debug)]
pub enum Base32Error {
    DecodeInvalidLen(usize),
    DecodeInvalidAscii(u8),
    DecodeOverflow(Vec<u8>),
}
impl fmt::Display for Base32Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Base32Error::DecodeInvalidAscii(b) => write!(f, "invalid ascii byte: {b}"),
            Base32Error::DecodeInvalidLen(len) => write!(f, "invalid length: {len}"),
            Base32Error::DecodeOverflow(bytes) => write!(f, "decode overflow: {bytes:X?}"),
        }
    }
}
impl core::error::Error for Base32Error {}
impl From<Base32Error> for Error {
    fn from(err: Base32Error) -> Self {
        Error::Base32Error(err)
    }
}
