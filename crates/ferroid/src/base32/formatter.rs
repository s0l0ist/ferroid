use core::{fmt, marker::PhantomData};

use super::encode_base32;
use crate::id::{BeBytes, Id};

/// A reusable builder that owns the Base32 buffer and formats an ID.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Base32Formatter<ID>
where
    ID: Id,
    ID::Ty: BeBytes,
{
    _id: PhantomData<ID>,
    buf: <ID::Ty as BeBytes>::Base32Array,
}

impl<ID: Id> Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    pub fn new(id: &ID) -> Self {
        let mut buf = <ID::Ty as BeBytes>::Base32Array::default();
        encode_base32::<ID::Ty>(&id.to_raw().to_be_bytes(), &mut buf);
        Self {
            _id: PhantomData,
            buf,
        }
    }

    /// Returns the underlying bytes of the base32 encoding.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.buf.as_ref()
    }

    /// Returns a `&str` view of the base32 encoding.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: Sealed BeBytes implementations use ordinary arrays. Encoding
        // overwrote every byte with ASCII; this shared borrow prevents mutation.
        unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Returns an allocated `String` of the base32 encoding.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn as_string(&self) -> alloc::string::String {
        // SAFETY: Sealed BeBytes implementations use ordinary arrays, and this
        // copies their fully initialized Crockford ASCII output.
        unsafe { alloc::string::String::from_utf8_unchecked(self.as_bytes().to_vec()) }
    }

    /// Consumes the formatter and returns the raw buffer.
    pub const fn into_inner(self) -> <ID::Ty as BeBytes>::Base32Array {
        self.buf
    }
}

impl<ID: Id> core::hash::Hash for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl<ID: Id> fmt::Display for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<ID: Id> fmt::Debug for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Base32Formatter")
            .field(&self.as_str())
            .finish()
    }
}

#[cfg(feature = "alloc")]
impl<ID: Id> From<&Base32Formatter<ID>> for alloc::string::String
where
    ID::Ty: BeBytes,
{
    fn from(formatter: &Base32Formatter<ID>) -> Self {
        formatter.as_string()
    }
}
#[cfg(feature = "alloc")]
impl<ID: Id> From<Base32Formatter<ID>> for alloc::string::String
where
    ID::Ty: BeBytes,
{
    fn from(formatter: Base32Formatter<ID>) -> Self {
        formatter.as_string()
    }
}

impl<ID: Id> AsRef<str> for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<ID: Id> AsRef<[u8]> for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<ID: Id> core::ops::Deref for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl<ID: Id> core::borrow::Borrow<str> for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<ID: Id> PartialEq<str> for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl<ID: Id> PartialEq<&str> for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

impl<ID: Id> PartialEq<Base32Formatter<ID>> for &str
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &Base32Formatter<ID>) -> bool {
        other == *self
    }
}

#[cfg(feature = "alloc")]
impl<ID: Id> PartialEq<alloc::string::String> for Base32Formatter<ID>
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &alloc::string::String) -> bool {
        self.as_str() == other.as_str()
    }
}
#[cfg(feature = "alloc")]
impl<ID: Id> PartialEq<Base32Formatter<ID>> for alloc::string::String
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &Base32Formatter<ID>) -> bool {
        self.as_str() == other.as_str()
    }
}

/// A builder that borrows a user-supplied buffer for Base32 formatting.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Base32FormatterRef<'a, ID>
where
    ID: Id,
    ID::Ty: BeBytes,
{
    _id: PhantomData<ID>,
    buf: &'a <ID::Ty as BeBytes>::Base32Array,
}

impl<'a, ID: Id> Base32FormatterRef<'a, ID>
where
    ID::Ty: BeBytes,
{
    pub fn new(id: &ID, buf: &'a mut <ID::Ty as BeBytes>::Base32Array) -> Self {
        encode_base32::<ID::Ty>(&id.to_raw().to_be_bytes(), buf);
        Self {
            _id: PhantomData,
            buf,
        }
    }

    /// Returns the underlying bytes of the base32 encoding.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        self.buf.as_ref()
    }

    /// Returns a `&str` view of the base32 encoding.
    #[must_use]
    pub fn as_str(&self) -> &str {
        // SAFETY: Sealed BeBytes implementations use ordinary arrays. Encoding
        // overwrote every byte with ASCII; this shared borrow prevents mutation.
        unsafe { core::str::from_utf8_unchecked(self.as_bytes()) }
    }

    /// Returns an allocated `String` of the base32 encoding.
    #[cfg(feature = "alloc")]
    #[must_use]
    pub fn as_string(&self) -> alloc::string::String {
        // SAFETY: Sealed BeBytes implementations use ordinary arrays, and this
        // copies their fully initialized Crockford ASCII output.
        unsafe { alloc::string::String::from_utf8_unchecked(self.as_bytes().to_vec()) }
    }
}

impl<ID: Id> core::hash::Hash for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl<ID: Id> fmt::Display for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<ID: Id> fmt::Debug for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Base32FormatterRef")
            .field(&self.as_str())
            .finish()
    }
}

#[cfg(feature = "alloc")]
impl<ID: Id> From<&Base32FormatterRef<'_, ID>> for alloc::string::String
where
    ID::Ty: BeBytes,
{
    fn from(formatter: &Base32FormatterRef<'_, ID>) -> Self {
        formatter.as_string()
    }
}
#[cfg(feature = "alloc")]
impl<ID: Id> From<Base32FormatterRef<'_, ID>> for alloc::string::String
where
    ID::Ty: BeBytes,
{
    fn from(formatter: Base32FormatterRef<'_, ID>) -> Self {
        formatter.as_string()
    }
}

impl<ID: Id> AsRef<str> for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl<ID: Id> AsRef<[u8]> for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl<ID: Id> core::ops::Deref for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    type Target = str;

    fn deref(&self) -> &str {
        self.as_str()
    }
}

impl<ID: Id> core::borrow::Borrow<str> for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

impl<ID: Id> PartialEq<str> for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}
impl<ID: Id> PartialEq<&str> for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}
impl<ID: Id> PartialEq<Base32FormatterRef<'_, ID>> for &str
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &Base32FormatterRef<'_, ID>) -> bool {
        other == *self
    }
}

#[cfg(feature = "alloc")]
impl<ID: Id> PartialEq<alloc::string::String> for Base32FormatterRef<'_, ID>
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &alloc::string::String) -> bool {
        self.as_str() == other.as_str()
    }
}
#[cfg(feature = "alloc")]
impl<ID: Id> PartialEq<Base32FormatterRef<'_, ID>> for alloc::string::String
where
    ID::Ty: BeBytes,
{
    fn eq(&self, other: &Base32FormatterRef<'_, ID>) -> bool {
        self.as_str() == other.as_str()
    }
}
