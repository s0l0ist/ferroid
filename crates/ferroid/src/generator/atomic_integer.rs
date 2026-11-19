use portable_atomic::Ordering;

mod sealed {
    pub trait Sealed {}

    #[cfg(target_has_atomic = "64")]
    impl Sealed for u64 {}

    #[cfg(target_has_atomic = "128")]
    impl Sealed for u128 {}
}

/// An ID backing integer with matching atomic storage.
///
/// Implemented for `u64` and `u128` when the target supports atomics of that
/// width. Atomic generators select the storage automatically from the ID's
/// backing type.
///
/// This trait is sealed and cannot be implemented outside this crate.
pub trait AtomicInteger: sealed::Sealed + Copy {
    /// The atomic type storing this integer without narrowing it.
    type Atomic: Send + Sync;

    /// Creates atomic storage initialized to `value`.
    #[must_use]
    fn new_atomic(value: Self) -> Self::Atomic;

    /// Loads the stored integer with relaxed ordering.
    #[must_use]
    fn load(state: &Self::Atomic) -> Self;

    /// Replaces `current` with `new` using relaxed ordering on success and failure.
    ///
    /// # Errors
    /// Returns the observed value if it differs from `current`.
    fn compare_exchange(state: &Self::Atomic, current: Self, new: Self) -> Result<Self, Self>;
}

macro_rules! impl_atomic_integer {
    ($int:ty, $atomic:ty) => {
        impl AtomicInteger for $int {
            type Atomic = $atomic;

            #[inline]
            fn new_atomic(value: Self) -> Self::Atomic {
                <$atomic>::new(value)
            }

            #[inline]
            fn load(state: &Self::Atomic) -> Self {
                state.load(Ordering::Relaxed)
            }

            #[inline]
            fn compare_exchange(
                state: &Self::Atomic,
                current: Self,
                new: Self,
            ) -> Result<Self, Self> {
                state.compare_exchange(current, new, Ordering::Relaxed, Ordering::Relaxed)
            }
        }
    };
}

#[cfg(target_has_atomic = "64")]
impl_atomic_integer!(u64, portable_atomic::AtomicU64);

#[cfg(target_has_atomic = "128")]
impl_atomic_integer!(u128, portable_atomic::AtomicU128);
