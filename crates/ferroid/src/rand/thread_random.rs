use rand::{Rng, RngExt, rng};

use crate::rand::{RandSource, RandomMask};

/// A `RandSource` that uses the thread-local RNG (`rand::rng()`).
///
/// This RNG is fast, cryptographically secure (ChaCha-based), and automatically
/// reseeded periodically.
///
/// Each OS thread has its own RNG instance, so calls from multiple threads are
/// contention-free and safe. This type does **not** store the RNG itself; it
/// simply accesses the thread-local generator on each call.
#[derive(Default, Clone, Debug)]
pub struct ThreadRandom;

impl RandSource<u64> for ThreadRandom {
    #[inline]
    fn rand<M: RandomMask<u64>>(&self) -> u64 {
        if M::MASK == 0 {
            return 0;
        }
        sample_u64::<M>(&mut rng())
    }
}

impl RandSource<u128> for ThreadRandom {
    #[inline]
    fn rand<M: RandomMask<u128>>(&self) -> u128 {
        if M::MASK == 0 {
            return 0;
        }
        sample_u128::<M>(&mut rng())
    }
}

#[inline]
fn sample_u64<M: RandomMask<u64>>(source: &mut (impl Rng + ?Sized)) -> u64 {
    let value = match const { u64::BITS - M::MASK.leading_zeros() } {
        0 => 0,
        1..=32 => u64::from(source.random::<u32>()),
        _ => source.random::<u64>(),
    };
    value & M::MASK
}

#[inline]
fn sample_u128<M: RandomMask<u128>>(source: &mut (impl Rng + ?Sized)) -> u128 {
    let value = match const { u128::BITS - M::MASK.leading_zeros() } {
        0 => 0,
        1..=32 => u128::from(source.random::<u32>()),
        33..=64 => u128::from(source.random::<u64>()),
        65..=96 => {
            let low = source.random::<u64>();
            let high = source.random::<u32>();
            u128::from(low) | (u128::from(high) << 64)
        }
        _ => source.random::<u128>(),
    };
    value & M::MASK
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::convert::Infallible;
    use rand::TryRng;

    struct Mask64<const MASK: u64>;
    impl<const MASK: u64> RandomMask<u64> for Mask64<MASK> {
        const MASK: u64 = MASK;
    }

    struct Mask128<const MASK: u128>;
    impl<const MASK: u128> RandomMask<u128> for Mask128<MASK> {
        const MASK: u128 = MASK;
    }

    #[derive(Default)]
    struct FixedRng {
        calls: usize,
    }

    impl TryRng for FixedRng {
        type Error = Infallible;

        fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
            self.calls += 1;
            Ok(0x89AB_CDEF)
        }

        fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
            self.calls += 1;
            Ok(0x0123_4567_89AB_CDEF)
        }

        fn try_fill_bytes(&mut self, _: &mut [u8]) -> Result<(), Self::Error> {
            panic!("word sampling should not fill a byte buffer");
        }
    }

    #[test]
    fn u64_masks_cover_word_boundaries() {
        fn check<const MASK: u64>(expected: u64) {
            assert_eq!(
                sample_u64::<Mask64<MASK>>(&mut FixedRng::default()),
                expected
            );
        }
        check::<1>(1);
        check::<0xFFFF>(0xCDEF);
        check::<0xFFFF_FFFF>(0x89AB_CDEF);
        check::<0x1_FFFF_FFFF>(0x1_89AB_CDEF);
        check::<{ u64::MAX }>(0x0123_4567_89AB_CDEF);
        check::<0x0100_0000_0000_00FF>(0x0100_0000_0000_00EF);
    }

    #[test]
    fn u128_masks_cover_word_boundaries() {
        fn check<const MASK: u128>(expected: u128) {
            assert_eq!(
                sample_u128::<Mask128<MASK>>(&mut FixedRng::default()),
                expected
            );
        }
        check::<1>(1);
        check::<0xFFFF>(0xCDEF);
        check::<0xFFFF_FFFF>(0x89AB_CDEF);
        check::<0x1_FFFF_FFFF>(0x1_89AB_CDEF);
        check::<{ (1u128 << 64) - 1 }>(0x0123_4567_89AB_CDEF);
        check::<{ (1u128 << 65) - 1 }>(0x1_0123_4567_89AB_CDEF);
        check::<{ (1u128 << 80) - 1 }>(0xCDEF_0123_4567_89AB_CDEF);
        check::<{ (1u128 << 96) - 1 }>(0x89AB_CDEF_0123_4567_89AB_CDEF);
        check::<{ (1u128 << 97) - 1 }>(0x1_89AB_CDEF_0123_4567_89AB_CDEF);
        check::<{ u128::MAX }>(0x0123_4567_89AB_CDEF_0123_4567_89AB_CDEF);
        check::<0x0100_0000_0000_0000_0000_0000_0000_00FF>(
            0x0100_0000_0000_0000_0000_0000_0000_00EF,
        );
    }

    #[test]
    fn zero_masks_do_not_draw_randomness() {
        let mut source = FixedRng::default();
        assert_eq!(sample_u64::<Mask64<0>>(&mut source), 0);
        assert_eq!(sample_u128::<Mask128<0>>(&mut source), 0);
        assert_eq!(source.calls, 0);
        let zero_u64: u64 = ThreadRandom.rand::<Mask64<0>>();
        let zero_u128: u128 = ThreadRandom.rand::<Mask128<0>>();
        assert_eq!(zero_u64, 0);
        assert_eq!(zero_u128, 0);
    }
}
