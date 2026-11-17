//! SSSE3 kernels for Crockford Base32.
//!
//! ```text
//! Encode: bytes -> five-bit symbols -> ASCII
//! Decode: ASCII -> five-bit symbols -> bytes
//! ```
//!
//! Encoding adds one leading padding bit for u64, or two for u128. Decoding
//! drops those bits. Invalid characters become 255; the caller discards the
//! result and locates the first error. Masks are generated at compile time.

use core::arch::x86_64::{
    __m128i, _mm_alignr_epi8, _mm_and_si128, _mm_andnot_si128, _mm_cmpeq_epi8, _mm_cmpgt_epi8,
    _mm_cvtsi32_si128, _mm_loadl_epi64, _mm_loadu_si128, _mm_madd_epi16, _mm_maddubs_epi16,
    _mm_movemask_epi8, _mm_mullo_epi16, _mm_or_si128, _mm_packus_epi16, _mm_set1_epi8,
    _mm_set1_epi16, _mm_set1_epi32, _mm_setzero_si128, _mm_shuffle_epi8, _mm_sll_epi16,
    _mm_slli_epi32, _mm_slli_si128, _mm_srl_epi16, _mm_srli_epi16, _mm_srli_si128,
    _mm_storel_epi64, _mm_storeu_si128, _mm_sub_epi8, _mm_unpacklo_epi64,
};

use super::common::{BITS_PER_CHAR, DECODE_COMPACT, ENCODE_LUT};
use crate::id::BeBytes;

const LANES: usize = 16;

// Leading padding is encoded characters * 5 - raw bytes * 8.
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // 13 * 5 - 64 = 1.
const U64_PADDING: i32 = (u64::BASE32_SIZE * BITS_PER_CHAR - u64::SIZE * 8) as i32;
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // 26 * 5 - 128 = 2.
const U128_PADDING: i32 = (u128::BASE32_SIZE * BITS_PER_CHAR - u128::SIZE * 8) as i32;

// Remove the three duplicate symbols; index 255 inserts zero.
const DECODE_U64_INDICES: [u8; LANES] = [0, 1, 2, 3, 4, 5, 6, 7, 11, 12, 13, 14, 15, 255, 255, 255];
// Two 16-byte windows over 26 characters overlap by six.
const DECODE_U128_OVERLAP: i32 = 6;

/// Encodes eight bytes as thirteen characters.
///
/// ```text
/// [8 bytes | zeros] -> [16 characters] -> copy first 13
/// ```
///
/// # Safety
///
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Fixed arrays provide 8 input bytes and 13 output bytes; full-vector
///   loads and stores use 16-byte staging arrays.
#[inline]
#[target_feature(enable = "ssse3")]
pub(super) fn encode64(input: &[u8; u64::SIZE], output: &mut [u8; u64::BASE32_SIZE]) {
    let mut raw = [0; LANES];
    raw[..u64::SIZE].copy_from_slice(input);
    let mut encoded = [0; LANES];
    // SAFETY:
    // - Both staged buffers hold a full vector;
    //   the final copy writes only the thirteen encoded characters.
    let raw = unsafe { _mm_loadu_si128(raw.as_ptr().cast()) };
    let symbols = encode_symbols(_mm_packus_epi16(
        extract_id_symbols::<u64, 0>(raw),
        extract_id_symbols::<u64, 8>(raw),
    ));
    // SAFETY:
    // - encoded is a live, exclusively borrowed 16-byte staging array.
    unsafe { _mm_storeu_si128(encoded.as_mut_ptr().cast(), symbols) };
    output.copy_from_slice(&encoded[..u64::BASE32_SIZE]);
}

/// Decodes thirteen characters, returning the value and an error flag.
///
/// ```text
/// [chars 0-7 | chars 5-12] -> lookup -> remove overlap -> pack -> 8 bytes
/// ```
///
/// All characters are checked before removing duplicates. Packing drops the
/// leading padding bit.
///
/// # Safety
///
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Loads cover input bytes 0..8 and 5..13; the output slice is checked
///   to hold 8 writable bytes before storing.
#[inline]
#[target_feature(enable = "ssse3")]
pub(super) fn decode64<T: BeBytes>(input: &[u8; u64::BASE32_SIZE]) -> (T, bool) {
    // SAFETY:
    // - Both eight-byte windows lie within the thirteen-byte input.
    let head = unsafe { _mm_loadl_epi64(input.as_ptr().cast()) };
    // SAFETY:
    // - The 8-byte load at offset 5 ends at byte 13 of input.
    let tail = unsafe { _mm_loadl_epi64(input.as_ptr().add(u64::BASE32_SIZE - u64::SIZE).cast()) };
    // Only invalid symbols have their high bit set.
    let symbols = decode_symbols(_mm_unpacklo_epi64(head, tail));
    let has_error = _mm_movemask_epi8(symbols) != 0;
    // SAFETY:
    // - The shuffle mask holds a full sixteen-byte vector.
    let symbols = _mm_shuffle_epi8(symbols, unsafe {
        _mm_loadu_si128(DECODE_U64_INDICES.as_ptr().cast())
    });
    let bytes = align_bytes::<U64_PADDING>(pack_symbols(symbols), _mm_setzero_si128());
    let mut decoded = T::ByteArray::default();
    let output = &mut decoded.as_mut()[..u64::SIZE];
    // SAFETY:
    // - The bounded slice provides eight writable bytes; storel writes
    //   only the low half of the vector and does not require vector alignment.
    unsafe { _mm_storel_epi64(output.as_mut_ptr().cast(), bytes) };
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
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Fixed arrays provide 16 input bytes and 26 output bytes; the final
///   partial vector is copied from a 16-byte staging array.
#[inline]
#[target_feature(enable = "ssse3")]
pub(super) fn encode128(input: &[u8; u128::SIZE], output: &mut [u8; u128::BASE32_SIZE]) {
    let mut encoded = [0; LANES];
    // SAFETY:
    // - Input and staging hold full vectors.
    //   The first store writes sixteen characters; the copy writes the last ten.
    let raw = unsafe { _mm_loadu_si128(input.as_ptr().cast()) };
    let first = encode_symbols(_mm_packus_epi16(
        extract_id_symbols::<u128, 0>(raw),
        extract_id_symbols::<u128, 8>(raw),
    ));
    // SAFETY:
    // - The fixed output array has 26 writable bytes; this store writes 16.
    unsafe { _mm_storeu_si128(output.as_mut_ptr().cast(), first) };
    let second = encode_symbols(_mm_packus_epi16(
        extract_id_symbols::<u128, 16>(raw),
        extract_id_symbols::<u128, 24>(raw),
    ));
    // SAFETY:
    // - encoded is a live, exclusively borrowed 16-byte staging array.
    unsafe { _mm_storeu_si128(encoded.as_mut_ptr().cast(), second) };
    output[LANES..].copy_from_slice(&encoded[..u128::BASE32_SIZE - LANES]);
}

/// Decodes twenty-six characters, returning the value and an error flag.
///
/// ```text
/// [chars 0-15 | chars 10-25] -> lookup -> remove overlap -> pack -> 16 bytes
/// ```
///
/// Pack each vector into ten bytes, join them, then drop two leading padding bits.
///
/// # Safety
///
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Loads cover input bytes 0..16 and 10..26; the output slice is checked
///   to hold 16 writable bytes before storing.
#[inline]
#[target_feature(enable = "ssse3")]
pub(super) fn decode128<T: BeBytes>(input: &[u8; u128::BASE32_SIZE]) -> (T, bool) {
    // SAFETY:
    // - Both sixteen-byte windows lie within the twenty-six-byte input.
    let first = decode_symbols(unsafe { _mm_loadu_si128(input.as_ptr().cast()) });
    // SAFETY:
    // - The 16-byte load at offset 10 ends at byte 26 of input.
    let second = decode_symbols(unsafe {
        _mm_loadu_si128(input.as_ptr().add(u128::BASE32_SIZE - LANES).cast())
    });
    let has_error = _mm_movemask_epi8(_mm_or_si128(first, second)) != 0;
    // Remove the six duplicate symbols from the second vector.
    let second = _mm_srli_si128::<DECODE_U128_OVERLAP>(second);
    let first = pack_symbols(first);
    let second = pack_symbols(second);
    // Join the packed bytes; retain the next byte for the padding shift.
    let packed_head = _mm_or_si128(first, _mm_slli_si128::<10>(second));
    let packed_next = _mm_srli_si128::<6>(second);
    let bytes = align_bytes::<U128_PADDING>(packed_head, packed_next);
    let mut decoded = T::ByteArray::default();
    let output = &mut decoded.as_mut()[..u128::SIZE];
    // SAFETY:
    // - The bounded slice provides sixteen writable bytes; storeu
    //   does not require vector alignment.
    unsafe { _mm_storeu_si128(output.as_mut_ptr().cast(), bytes) };
    (T::from_be_bytes(decoded), has_error)
}

/// Extracts eight symbols into u16 lanes, starting at character index `START`.
///
/// # Safety
///
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Both generated masks contain 16 initialized bytes.
#[inline]
#[target_feature(enable = "ssse3")]
fn extract_id_symbols<T: BeBytes, const START: usize>(input: __m128i) -> __m128i {
    // SAFETY:
    // - Both constant tables contain sixteen bytes.
    extract_symbols(
        input,
        // SAFETY:
        // - The generated mask has 16 initialized bytes, enough for this load.
        unsafe { _mm_loadu_si128(const { encode_indices::<T>(START) }.as_ptr().cast()) },
        // SAFETY:
        // - The generated mask has 16 initialized bytes, enough for this load.
        unsafe { _mm_loadu_si128(const { encode_multipliers::<T>(START) }.as_ptr().cast()) },
    )
}

/// Extracts five bits from each adjacent-byte window.
///
/// ```text
/// [next, first] -> u16 * 2^offset -> keep low 16 bits -> >> 11
/// ```
///
/// Multiplication supplies the per-lane shifts that SSSE3 lacks.
///
/// # Safety
///
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Operations use only vector registers and do not dereference memory.
#[inline]
#[target_feature(enable = "ssse3")]
fn extract_symbols(input: __m128i, indices: __m128i, multipliers: __m128i) -> __m128i {
    let windows = _mm_shuffle_epi8(input, indices);
    _mm_srli_epi16::<11>(_mm_mullo_epi16(windows, multipliers))
}

/// Maps five-bit symbols through the two halves of the alphabet.
///
/// ```text
/// symbol  0-15 -> low half  -> ASCII
/// symbol 16-31 -> high half -> ASCII
/// ```
///
/// Both shuffles use the low four bits; a mask selects the matching half.
///
/// # Safety
///
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Alphabet loads stay within the 32-byte static table.
#[inline]
#[target_feature(enable = "ssse3")]
fn encode_symbols(symbols: __m128i) -> __m128i {
    // SAFETY:
    // - Both loads stay within the 32-byte alphabet.
    let low = unsafe { _mm_loadu_si128(ENCODE_LUT.as_ptr().cast()) };
    // SAFETY:
    // - The 16-byte load at offset 16 stays within the 32-byte static alphabet.
    let high = unsafe { _mm_loadu_si128(ENCODE_LUT.as_ptr().add(16).cast()) };
    let use_high = _mm_cmpgt_epi8(symbols, _mm_set1_epi8(15));
    let low = _mm_shuffle_epi8(low, symbols);
    let high = _mm_shuffle_epi8(high, symbols);
    _mm_or_si128(
        _mm_andnot_si128(use_high, low),
        _mm_and_si128(use_high, high),
    )
}

/// Maps ASCII to 0..31, or 255 for invalid input.
///
/// ```text
/// uppercase -> subtract '0' -> table (symbol + 1) -> subtract 1
/// ```
///
/// # Safety
///
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Lookup loads stay within the 48-byte static table.
#[inline]
#[target_feature(enable = "ssse3")]
fn decode_symbols(input: __m128i) -> __m128i {
    // Clear ASCII case bit 5 only where bit 6 is set, preserving digits.
    // The bit 5 mask also removes carries from the word-sized shift.
    let case_bit = _mm_and_si128(_mm_srli_epi16::<1>(input), _mm_set1_epi8(32));
    let uppercase = _mm_andnot_si128(case_bit, input);
    let indices = _mm_sub_epi8(uppercase, _mm_set1_epi8(b'0'.cast_signed()));

    // Gate each table lookup by its high nibble; other blocks contribute zero.
    let block = _mm_and_si128(indices, _mm_set1_epi8(-16));
    // SAFETY:
    // - The three sixteen-byte loads cover the 48-byte compact table.
    let low = unsafe { _mm_loadu_si128(DECODE_COMPACT.as_ptr().cast()) };
    // SAFETY:
    // - The 16-byte load at offset 16 stays within the 48-byte static table.
    let middle = unsafe { _mm_loadu_si128(DECODE_COMPACT.as_ptr().add(16).cast()) };
    // SAFETY:
    // - The 16-byte load at offset 32 stays within the 48-byte static table.
    let high = unsafe { _mm_loadu_si128(DECODE_COMPACT.as_ptr().add(32).cast()) };
    let low = _mm_and_si128(
        _mm_shuffle_epi8(low, indices),
        _mm_cmpeq_epi8(block, _mm_setzero_si128()),
    );
    let middle = _mm_and_si128(
        _mm_shuffle_epi8(middle, indices),
        _mm_cmpeq_epi8(block, _mm_set1_epi8(16)),
    );
    let high = _mm_and_si128(
        _mm_shuffle_epi8(high, indices),
        _mm_cmpeq_epi8(block, _mm_set1_epi8(32)),
    );

    _mm_sub_epi8(
        _mm_or_si128(_mm_or_si128(low, middle), high),
        _mm_set1_epi8(1),
    )
}

/// Packs sixteen symbols into ten bytes, followed by six zero bytes.
///
/// ```text
/// 16 x 5 bits -> 8 x 10 bits -> 4 x 20 bits -> 10 bytes
///               maddubs        madd          shuffle + OR
/// ```
///
/// Valid symbols keep both multiply-adds within range.
///
/// # Safety
///
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Both shuffle masks contain 16 initialized bytes.
#[inline]
#[target_feature(enable = "ssse3")]
fn pack_symbols(symbols: __m128i) -> __m128i {
    const EVEN: [u8; LANES] = [
        2, 1, 0, 128, 128, 10, 9, 8, 128, 128, 128, 128, 128, 128, 128, 128,
    ];
    const ODD: [u8; LANES] = [
        128, 128, 6, 5, 4, 128, 128, 14, 13, 12, 128, 128, 128, 128, 128, 128,
    ];
    // Join pairs of symbols, then pairs of pairs.
    let pairs = _mm_maddubs_epi16(symbols, _mm_set1_epi16(0x0120));
    let quads = _mm_madd_epi16(pairs, _mm_set1_epi32(0x0001_0400));
    // EVEN and ODD place alternating 20-bit groups into consecutive bytes.
    // SAFETY:
    // - Both shuffle masks contain sixteen bytes.
    _mm_or_si128(
        // SAFETY:
        // - The shuffle mask has 16 initialized bytes; the unaligned load reads exactly 16.
        _mm_shuffle_epi8(_mm_slli_epi32::<4>(quads), unsafe {
            _mm_loadu_si128(EVEN.as_ptr().cast())
        }),
        // SAFETY:
        // - The shuffle mask has 16 initialized bytes; the unaligned load reads exactly 16.
        _mm_shuffle_epi8(quads, unsafe { _mm_loadu_si128(ODD.as_ptr().cast()) }),
    )
}

/// Drops one (u64) or two (u128) leading padding bits.
///
/// ```text
/// [padding | payload bits ...] -> [payload bits ...]
/// ```
///
/// Shift each byte left and carry in the high bits of the following byte.
/// Per-byte masks remove carries introduced by the u16 shifts.
///
/// # Safety
///
/// - The caller must run on a CPU with SSSE3 enabled.
/// - Operations use only vector registers and do not dereference memory.
#[inline]
#[target_feature(enable = "ssse3")]
fn align_bytes<const PADDING: i32>(head: __m128i, next: __m128i) -> __m128i {
    let following = _mm_alignr_epi8::<1>(next, head);
    let left = _mm_sll_epi16(head, _mm_cvtsi32_si128(PADDING));
    let right = _mm_srl_epi16(following, _mm_cvtsi32_si128(8 - PADDING));
    _mm_or_si128(
        _mm_and_si128(left, _mm_set1_epi8(-1_i8 << PADDING)),
        _mm_and_si128(right, _mm_set1_epi8(!(-1_i8 << PADDING))),
    )
}

// Gather adjacent bytes as little-endian u16 windows. Shuffle index 128
// supplies zero for bytes outside the input, including leading padding.
#[allow(clippy::cast_possible_truncation)] // The guards bound both cast values to 0..=15.
const fn encode_indices<T: BeBytes>(start: usize) -> [u8; LANES] {
    let padding = T::BASE32_SIZE * BITS_PER_CHAR - T::SIZE * 8;
    let mut indices = [128; LANES];
    let mut lane = 0;
    while lane < 8 {
        let next = ((start + lane) * 5 + 8 - padding) / 8;
        if next < LANES {
            indices[lane * 2] = next as u8;
        }
        if next > 0 && next <= LANES {
            indices[lane * 2 + 1] = (next - 1) as u8;
        }
        lane += 1;
    }
    indices
}

// Turn each symbol's bit offset into the corresponding power-of-two shift.
const fn encode_multipliers<T: BeBytes>(start: usize) -> [u16; 8] {
    let padding = T::BASE32_SIZE * BITS_PER_CHAR - T::SIZE * 8;
    let mut multipliers = [0; 8];
    let mut lane = 0;
    while lane < 8 {
        let offset = ((start + lane) * 5 + 8 - padding) % 8;
        multipliers[lane] = 1 << offset;
        lane += 1;
    }
    multipliers
}
