//! NEON kernels for Crockford Base32.
//!
//! ```text
//! Encode: bytes -> five-bit symbols -> ASCII
//! Decode: ASCII -> five-bit symbols -> bytes
//! ```
//!
//! Encoding adds one leading padding bit for u64, or two for u128. Decoding
//! drops those bits. Invalid characters become 255; the caller discards the
//! result and locates the first error. Masks are generated at compile time.

use core::arch::aarch64::{
    uint8x16_t, uint8x16x2_t, uint8x16x3_t, vadd_u8, vaddq_u8, vandq_u8, vbicq_u8, vcombine_u8,
    vdup_n_u8, vdupq_n_s16, vdupq_n_u8, vdupq_n_u16, vld1_u8, vld1q_u8, vmaxvq_u8, vmovl_u8,
    vmovn_u16, vorr_u8, vorrq_u8, vqtbl1_u8, vqtbl1q_u8, vqtbl2q_u8, vqtbl3q_u8,
    vreinterpret_s8_u8, vreinterpretq_s8_u8, vreinterpretq_s8_u16, vreinterpretq_s16_u16,
    vreinterpretq_u16_u8, vshl_u8, vshlq_u8, vshlq_u16, vshrq_n_u8, vst1_u8, vst1q_u8, vsubq_s16,
    vsubq_u8,
};

use super::common::{BITS_PER_CHAR, DECODE_COMPACT, ENCODE_LUT, INVALID_VALUE};
use crate::id::BeBytes;

const LANES: usize = 16;

/// Encodes eight bytes as thirteen characters.
///
/// ```text
/// [8 bytes | zeros] -> [16 characters] -> copy first 13
/// ```
///
/// # Safety
///
/// - The caller must run on a CPU with NEON enabled.
/// - Fixed arrays provide 8 input bytes and 13 output bytes; full-vector
///   loads and stores use 16-byte staging arrays.
#[inline]
#[target_feature(enable = "neon")]
pub(super) fn encode64(input: &[u8; u64::SIZE], output: &mut [u8; u64::BASE32_SIZE]) {
    let mut raw = [0; LANES];
    raw[..u64::SIZE].copy_from_slice(input);
    let mut encoded = [0; LANES];
    // SAFETY:
    // - Both staged buffers and both masks hold a full vector;
    //   the final copy writes only the thirteen encoded characters.
    let raw = unsafe { vld1q_u8(raw.as_ptr()) };
    let symbols = encode_symbols(
        raw,
        // SAFETY:
        // - The generated mask has 16 initialized bytes, enough for this load.
        unsafe { vld1q_u8(const { encode_indices::<u64>(0) }.as_ptr()) },
        // SAFETY:
        // - The generated mask has 16 initialized bytes, enough for this load.
        unsafe { vld1q_u8(const { encode_offsets::<u64>(0) }.as_ptr()) },
    );
    // SAFETY:
    // - encoded is a live, exclusively borrowed 16-byte staging array.
    unsafe { vst1q_u8(encoded.as_mut_ptr(), symbols) };
    output.copy_from_slice(&encoded[..u64::BASE32_SIZE]);
}

/// Decodes thirteen characters, returning the value and an error flag.
///
/// ```text
/// [chars 0-7 | chars 5-12] -> lookup -> gather + pack -> 8 bytes
/// ```
///
/// Masks skip duplicate symbols. Two gathers supply each byte's first symbol
/// and a pair of following symbols, packed together in a u16 lane.
///
/// # Safety
///
/// - The caller must run on a CPU with NEON enabled.
/// - Loads cover input bytes 0..8 and 5..13; the output slice is checked
///   to hold 8 writable bytes before storing.
#[inline]
#[target_feature(enable = "neon")]
pub(super) fn decode64<T: BeBytes>(input: &[u8; u64::BASE32_SIZE]) -> (T, bool) {
    // SAFETY:
    // - Both eight-byte windows lie within the thirteen-byte input.
    let head = unsafe { vld1_u8(input.as_ptr()) };
    // SAFETY:
    // - The 8-byte load at offset 5 ends at byte 13 of input.
    let tail = unsafe { vld1_u8(input.as_ptr().add(u64::BASE32_SIZE - u64::SIZE)) };
    let symbols = decode_symbols(vcombine_u8(head, tail));
    let has_error = vmaxvq_u8(symbols) == INVALID_VALUE;
    // SAFETY:
    // - Each mask contains sixteen bytes; these loads read eight or sixteen.
    let offset = unsafe { vld1_u8(const { decode_offsets::<u64>() }.as_ptr()) };
    // SAFETY:
    // - The generated mask has 16 initialized bytes, enough for this load.
    let first = vqtbl1_u8(symbols, unsafe {
        vld1_u8(const { decode_indices::<u64>(0) }.as_ptr())
    });
    // SAFETY:
    // - The generated mask has 16 initialized bytes, enough for this load.
    let windows = vqtbl1q_u8(symbols, unsafe {
        vld1q_u8(const { decode64_pair_indices() }.as_ptr())
    });
    // Each u16 holds (b << 8) | c. Shift only c by three; its five bits
    // still fit in one byte. Native byte order keeps b unchanged.
    let windows = vshlq_u8(windows, vreinterpretq_s8_u16(vdupq_n_u16(3)));
    // Align the pair with the first symbol, retaining only the low byte.
    let word_shift = vsubq_s16(vreinterpretq_s16_u16(vmovl_u8(offset)), vdupq_n_s16(10));
    let rest = vmovn_u16(vshlq_u16(vreinterpretq_u16_u8(windows), word_shift));
    let first = vshl_u8(first, vreinterpret_s8_u8(vadd_u8(offset, vdup_n_u8(3))));
    let mut decoded = T::ByteArray::default();
    let output = &mut decoded.as_mut()[..u64::SIZE];
    // SAFETY:
    // - The bounded slice provides eight writable bytes.
    unsafe { vst1_u8(output.as_mut_ptr(), vorr_u8(first, rest)) };
    (T::from_be_bytes(decoded), has_error)
}

/// Encodes sixteen bytes as twenty-six characters from the same input vector.
///
/// ```text
/// 16 bytes -> [characters 0-15] [characters 16-25 | unused]
///                  store 16          copy 10
/// ```
///
/// # Safety
///
/// - The caller must run on a CPU with NEON enabled.
/// - Fixed arrays provide 16 input bytes and 26 output bytes; the final
///   partial vector is copied from a 16-byte staging array.
#[inline]
#[target_feature(enable = "neon")]
pub(super) fn encode128(input: &[u8; u128::SIZE], output: &mut [u8; u128::BASE32_SIZE]) {
    let mut encoded = [0; LANES];
    // SAFETY:
    // - Input, masks, and staging hold full vectors.
    //   The first store writes sixteen characters; the copy writes the last ten.
    let raw = unsafe { vld1q_u8(input.as_ptr()) };
    let first = encode_symbols(
        raw,
        // SAFETY:
        // - The generated mask has 16 initialized bytes, enough for this load.
        unsafe { vld1q_u8(const { encode_indices::<u128>(0) }.as_ptr()) },
        // SAFETY:
        // - The generated mask has 16 initialized bytes, enough for this load.
        unsafe { vld1q_u8(const { encode_offsets::<u128>(0) }.as_ptr()) },
    );
    // SAFETY:
    // - The fixed output array has 26 writable bytes; this store writes 16.
    unsafe { vst1q_u8(output.as_mut_ptr(), first) };
    let second = encode_symbols(
        raw,
        // SAFETY:
        // - The generated mask has 16 initialized bytes, enough for this load.
        unsafe { vld1q_u8(const { encode_indices::<u128>(LANES) }.as_ptr()) },
        // SAFETY:
        // - The generated mask has 16 initialized bytes, enough for this load.
        unsafe { vld1q_u8(const { encode_offsets::<u128>(LANES) }.as_ptr()) },
    );
    // SAFETY:
    // - encoded is a live, exclusively borrowed 16-byte staging array.
    unsafe { vst1q_u8(encoded.as_mut_ptr(), second) };
    output[LANES..].copy_from_slice(&encoded[..u128::BASE32_SIZE - LANES]);
}

/// Decodes twenty-six characters, returning the value and an error flag.
///
/// ```text
/// [chars 0-15 | chars 10-25] -> lookup -> gather + pack -> 16 bytes
/// ```
///
/// Masks skip duplicate symbols and gather up to three symbols per output byte.
///
/// # Safety
///
/// - The caller must run on a CPU with NEON enabled.
/// - Loads cover input bytes 0..16 and 10..26; the output slice is checked
///   to hold 16 writable bytes before storing.
#[inline]
#[target_feature(enable = "neon")]
pub(super) fn decode128<T: BeBytes>(input: &[u8; u128::BASE32_SIZE]) -> (T, bool) {
    // SAFETY:
    // - Both sixteen-byte windows lie within the twenty-six-byte input.
    let first = decode_symbols(unsafe { vld1q_u8(input.as_ptr()) });
    // SAFETY:
    // - The 16-byte load at offset 10 ends at byte 26 of input.
    let second = decode_symbols(unsafe { vld1q_u8(input.as_ptr().add(u128::BASE32_SIZE - LANES)) });
    let has_error = vmaxvq_u8(vorrq_u8(first, second)) == INVALID_VALUE;
    let symbols = uint8x16x2_t(first, second);
    // SAFETY:
    // - The generated mask has 16 initialized bytes, enough for this load.
    let first = vqtbl2q_u8(symbols, unsafe {
        vld1q_u8(const { decode_indices::<u128>(0) }.as_ptr())
    });
    // SAFETY:
    // - The generated mask has 16 initialized bytes, enough for this load.
    let second = vqtbl2q_u8(symbols, unsafe {
        vld1q_u8(const { decode_indices::<u128>(1) }.as_ptr())
    });
    // SAFETY:
    // - The generated mask has 16 initialized bytes, enough for this load.
    let third = vqtbl2q_u8(symbols, unsafe {
        vld1q_u8(const { decode_indices::<u128>(2) }.as_ptr())
    });
    // SAFETY:
    // - The generated mask has 16 initialized bytes, enough for this load.
    let bytes = pack_bytes(first, second, third, unsafe {
        vld1q_u8(const { decode_offsets::<u128>() }.as_ptr())
    });
    let mut decoded = T::ByteArray::default();
    let output = &mut decoded.as_mut()[..u128::SIZE];
    // SAFETY:
    // - The bounded slice provides sixteen writable bytes.
    unsafe { vst1q_u8(output.as_mut_ptr(), bytes) };
    (T::from_be_bytes(decoded), has_error)
}

/// Extracts five-bit symbols from adjacent bytes and looks up their characters.
///
/// ```text
/// two bytes -> shift each -> OR -> mask with 31 -> alphabet
/// ```
///
/// NEON shifts use signed counts: negative means right. Table index 255 supplies
/// the virtual zero byte before the input; adding one wraps to index zero.
///
/// # Safety
///
/// - The caller must run on a CPU with NEON enabled.
/// - Alphabet loads stay within the 32-byte static table.
#[inline]
#[target_feature(enable = "neon")]
fn encode_symbols(input: uint8x16_t, indices: uint8x16_t, offset: uint8x16_t) -> uint8x16_t {
    let first = vqtbl1q_u8(input, indices);
    let second = vqtbl1q_u8(input, vaddq_u8(indices, vdupq_n_u8(1)));
    let first = vshlq_u8(first, vreinterpretq_s8_u8(vsubq_u8(offset, vdupq_n_u8(3))));
    let second = vshlq_u8(
        second,
        vreinterpretq_s8_u8(vsubq_u8(offset, vdupq_n_u8(11))),
    );
    let symbols = vandq_u8(vorrq_u8(first, second), vdupq_n_u8(31));
    // SAFETY:
    // - The two sixteen-byte loads cover the thirty-two-byte alphabet.
    let alphabet = uint8x16x2_t(unsafe { vld1q_u8(ENCODE_LUT.as_ptr()) }, unsafe {
        vld1q_u8(ENCODE_LUT.as_ptr().add(16))
    });
    vqtbl2q_u8(alphabet, symbols)
}

/// Maps ASCII to 0..31, or 255 for invalid input.
///
/// ```text
/// uppercase -> subtract '0' -> table (symbol + 1) -> subtract 1
/// ```
///
/// # Safety
///
/// - The caller must run on a CPU with NEON enabled.
/// - Lookup loads stay within the 48-byte static table.
#[inline]
#[target_feature(enable = "neon")]
fn decode_symbols(input: uint8x16_t) -> uint8x16_t {
    // Clear ASCII case bit 5 only where bit 6 is set, preserving digits.
    let case_bit = vandq_u8(vshrq_n_u8::<1>(input), vdupq_n_u8(32));
    let uppercase = vbicq_u8(input, case_bit);
    let indices = vsubq_u8(uppercase, vdupq_n_u8(b'0'));
    // SAFETY:
    // - The three sixteen-byte loads cover the compact table.
    let table = uint8x16x3_t(
        // SAFETY:
        // - The 16-byte load at offset 0 stays within the 48-byte static table.
        unsafe { vld1q_u8(DECODE_COMPACT.as_ptr()) },
        // SAFETY:
        // - The 16-byte load at offset 16 stays within the 48-byte static table.
        unsafe { vld1q_u8(DECODE_COMPACT.as_ptr().add(16)) },
        // SAFETY:
        // - The 16-byte load at offset 32 stays within the 48-byte static table.
        unsafe { vld1q_u8(DECODE_COMPACT.as_ptr().add(32)) },
    );
    vsubq_u8(vqtbl3q_u8(table, indices), vdupq_n_u8(1))
}

/// Aligns up to three five-bit symbols into each output byte.
///
/// ```text
/// [aaaaa bbbbb ccccc] -> skip offset bits -> [eight output bits]
/// ```
///
/// Signed shifts align each contribution; OR joins them in byte-sized lanes.
///
/// # Safety
///
/// - The caller must run on a CPU with NEON enabled.
/// - Operations use only vector registers and do not dereference memory.
#[inline]
#[target_feature(enable = "neon")]
fn pack_bytes(
    first: uint8x16_t,
    second: uint8x16_t,
    third: uint8x16_t,
    offset: uint8x16_t,
) -> uint8x16_t {
    let first = vshlq_u8(first, vreinterpretq_s8_u8(vaddq_u8(offset, vdupq_n_u8(3))));
    let second = vshlq_u8(second, vreinterpretq_s8_u8(vsubq_u8(offset, vdupq_n_u8(2))));
    let third = vshlq_u8(third, vreinterpretq_s8_u8(vsubq_u8(offset, vdupq_n_u8(7))));
    vorrq_u8(vorrq_u8(first, second), third)
}

// Symbol i starts at bit 5*i - padding. Index 255 supplies a leading zero.
// Supported widths and starts bound the value before wrapping to 0..=20.
#[allow(clippy::cast_possible_truncation)]
const fn encode_indices<T: BeBytes>(start: usize) -> [u8; LANES] {
    let padding = T::BASE32_SIZE * BITS_PER_CHAR - T::SIZE * 8;
    let mut indices = [0; LANES];
    let mut lane = 0;
    while lane < LANES {
        indices[lane] = (((start + lane) * 5 + 8 - padding) / 8) as u8;
        indices[lane] = indices[lane].wrapping_sub(1);
        lane += 1;
    }
    indices
}

// Bit position of each symbol within its adjacent-byte window.
#[allow(clippy::cast_possible_truncation)] // Modulo 8 bounds the cast value to 0..=7.
const fn encode_offsets<T: BeBytes>(start: usize) -> [u8; LANES] {
    let padding = T::BASE32_SIZE * BITS_PER_CHAR - T::SIZE * 8;
    let mut offsets = [0; LANES];
    let mut lane = 0;
    while lane < LANES {
        offsets[lane] = (((start + lane) * 5 + 8 - padding) % 8) as u8;
        lane += 1;
    }
    offsets
}

// Map each byte's symbols into overlapping loads, skipping duplicates.
// Supported arguments bound indices to 0..=32; out-of-range lookups give zero.
#[allow(clippy::cast_possible_truncation)]
const fn decode_indices<T: BeBytes>(adjacent: usize) -> [u8; LANES] {
    let padding = T::BASE32_SIZE * BITS_PER_CHAR - T::SIZE * 8;
    let overlap = T::SIZE * 2 - T::BASE32_SIZE;
    let mut indices = [0; LANES];
    let mut lane = 0;
    while lane < LANES {
        let symbol = (padding + lane * 8) / BITS_PER_CHAR + adjacent;
        indices[lane] = (if symbol < T::SIZE {
            symbol
        } else {
            symbol + overlap
        }) as u8;
        lane += 1;
    }
    indices
}

// Number of high bits to skip in each byte's first symbol, including padding.
#[allow(clippy::cast_possible_truncation)] // Modulo 5 bounds the cast value to 0..=4.
const fn decode_offsets<T: BeBytes>() -> [u8; LANES] {
    let padding = T::BASE32_SIZE * BITS_PER_CHAR - T::SIZE * 8;
    let mut offsets = [0; LANES];
    let mut lane = 0;
    while lane < LANES {
        offsets[lane] = ((padding + lane * 8) % 5) as u8;
        lane += 1;
    }
    offsets
}

// Gather symbol pairs as (second << 8) | third in native u16 byte order.
const fn decode64_pair_indices() -> [u8; LANES] {
    let second = decode_indices::<u64>(1);
    let third = decode_indices::<u64>(2);
    let mut indices = [0; LANES];
    let mut lane = 0;
    while lane < u64::SIZE {
        let bytes = u16::from_be_bytes([second[lane], third[lane]]).to_ne_bytes();
        indices[lane * 2] = bytes[0];
        indices[lane * 2 + 1] = bytes[1];
        lane += 1;
    }
    indices
}
