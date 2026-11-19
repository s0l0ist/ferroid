/// The random field mask supplied by an ID layout.
///
/// `define_ulid!` implements this automatically. You only need to reference
/// this trait when implementing a custom [`RandSource`].
pub trait RandomMask<T> {
    /// The bit positions that a random draw may set.
    const MASK: T;
}

/// A source of random bits for an ID layout.
///
/// The backing type `T` is typically `u64` or `u128`. Generators select the
/// ID layout, and `define_ulid!` supplies its random field mask automatically.
///
/// # Implementing a custom source
/// ```
/// # #[cfg(feature = "ulid")]
/// # {
/// use ferroid::define_ulid;
/// use ferroid::rand::{RandSource, RandomMask};
///
/// define_ulid!(ExampleId, u64, reserved: 0, timestamp: 56, random: 8);
///
/// struct FixedRand;
/// impl RandSource<u64> for FixedRand {
///     fn rand<M: RandomMask<u64>>(&self) -> u64 {
///         0x04D2 & M::MASK
///     }
/// }
///
/// let rng = FixedRand;
/// assert_eq!(rng.rand::<ExampleId>(), 0xD2);
/// # }
/// ```
pub trait RandSource<T> {
    /// Returns random bits only in positions selected by `M::MASK`.
    ///
    /// Implementations must clear bits outside the layout's random field using
    /// `M::MASK`. The mask is known at compile time.
    fn rand<M: RandomMask<T>>(&self) -> T;
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ops::BitAnd;

    struct FixedRand<T>(T);
    impl<T: Copy + BitAnd<Output = T>> RandSource<T> for FixedRand<T> {
        fn rand<M: RandomMask<T>>(&self) -> T {
            self.0 & M::MASK
        }
    }

    struct Mask<const MASK: u128>;
    impl<const MASK: u128> RandomMask<u128> for Mask<MASK> {
        const MASK: u128 = MASK;
    }

    #[test]
    fn custom_sources_respect_mask_types() {
        fn check<const MASK: u128>() {
            assert_eq!(FixedRand(u128::MAX).rand::<Mask<MASK>>(), MASK);
        }
        check::<0>();
        check::<1>();
        check::<{ (1u128 << 64) - 1 }>();
        check::<{ u128::MAX }>();
        check::<0xA005>();
    }
}
