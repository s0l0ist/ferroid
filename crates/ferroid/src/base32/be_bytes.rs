use std::hash::Hash;

const fn base32_size(bytes: usize) -> usize {
    (bytes * 8).div_ceil(5)
}

/// A trait for types that can be encoded to and decoded from big-endian bytes.
pub trait BeBytes: Sized {
    const SIZE: usize;
    const BASE32_SIZE: usize;
    type ByteArray: AsRef<[u8]>
        + AsMut<[u8]>
        + core::fmt::Debug
        + Default
        + Copy
        + PartialEq
        + Eq
        + PartialOrd
        + Ord
        + Hash;
    type Base32Array: AsRef<[u8]>
        + AsMut<[u8]>
        + core::fmt::Debug
        + Default
        + Copy
        + PartialEq
        + Eq
        + PartialOrd
        + Ord
        + Hash;

    fn to_be_bytes(self) -> Self::ByteArray;
    fn from_be_bytes(bytes: Self::ByteArray) -> Self;
}
impl BeBytes for u32 {
    const SIZE: usize = core::mem::size_of::<u32>();
    const BASE32_SIZE: usize = base32_size(Self::SIZE);

    type ByteArray = [u8; Self::SIZE];
    type Base32Array = [u8; Self::BASE32_SIZE];

    fn to_be_bytes(self) -> Self::ByteArray {
        self.to_be_bytes()
    }

    fn from_be_bytes(bytes: Self::ByteArray) -> Self {
        Self::from_be_bytes(bytes)
    }
}
impl BeBytes for u64 {
    const SIZE: usize = core::mem::size_of::<u64>();
    const BASE32_SIZE: usize = base32_size(Self::SIZE);

    type ByteArray = [u8; Self::SIZE];
    type Base32Array = [u8; Self::BASE32_SIZE];

    fn to_be_bytes(self) -> Self::ByteArray {
        self.to_be_bytes()
    }

    fn from_be_bytes(bytes: Self::ByteArray) -> Self {
        Self::from_be_bytes(bytes)
    }
}
impl BeBytes for u128 {
    const SIZE: usize = core::mem::size_of::<u128>();
    const BASE32_SIZE: usize = base32_size(Self::SIZE);

    type ByteArray = [u8; Self::SIZE];
    type Base32Array = [u8; Self::BASE32_SIZE];

    fn to_be_bytes(self) -> Self::ByteArray {
        self.to_be_bytes()
    }

    fn from_be_bytes(bytes: Self::ByteArray) -> Self {
        Self::from_be_bytes(bytes)
    }
}
