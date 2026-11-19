use crate::{
    rand::{RandSource, RandomMask},
    time::TimeSource,
};
use core::{cell::Cell, num::NonZeroU64};

pub(super) struct CountingRand {
    next: Cell<u64>,
}

impl CountingRand {
    pub(super) fn new() -> Self {
        Self { next: Cell::new(0) }
    }
}

impl<T: From<u64> + core::ops::BitAnd<Output = T>> RandSource<T> for CountingRand {
    fn rand<M: RandomMask<T>>(&self) -> T {
        let next = self.next.get();
        self.next.set(next + 1);
        T::from(next) & M::MASK
    }
}

pub(super) struct ZeroRand;
impl<T: From<u64> + core::ops::BitAnd<Output = T>> RandSource<T> for ZeroRand {
    fn rand<M: RandomMask<T>>(&self) -> T {
        T::from(0) & M::MASK
    }
}

#[cfg(any(feature = "basic", feature = "lock", feature = "std"))]
pub(super) struct MockTime {
    pub(super) millis: u64,
}

#[cfg(any(feature = "basic", feature = "lock", feature = "std"))]
impl<T: From<u64>> TimeSource<T> for MockTime {
    fn current_millis(&self) -> T {
        T::from(self.millis)
    }
}

pub(super) struct TestClock {
    pub(super) ticks: Cell<u64>,
}

impl TestClock {
    pub(super) fn new(ticks: u64) -> Self {
        Self {
            ticks: Cell::new(ticks),
        }
    }
}

impl<T: From<u64>> TimeSource<T> for &TestClock {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(10).unwrap();

    fn current_millis(&self) -> T {
        T::from(self.ticks.get())
    }
}

pub(super) struct OverflowClock;
impl<T: From<u64>> TimeSource<T> for OverflowClock {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::MAX;

    fn current_millis(&self) -> T {
        T::from(0)
    }
}

#[cfg(any(
    feature = "basic",
    feature = "lock",
    all(feature = "atomic", target_has_atomic = "128")
))]
#[test]
fn generators_request_the_layout_random_mask() {
    use crate::{generator::*, id::ULID};

    struct MaskOnlyRand;
    impl RandSource<u128> for MaskOnlyRand {
        fn rand<M: crate::rand::RandomMask<u128>>(&self) -> u128 {
            assert_eq!(M::MASK, ULID::max_random());
            42
        }
    }

    let clock = TestClock::new(1);
    let expected = Poll::Ready {
        id: ULID::from_components(1, 42),
    };
    #[cfg(feature = "basic")]
    {
        let basic = BasicUlidGenerator::<ULID, _, _>::new(&clock, MaskOnlyRand);
        assert_eq!(basic.try_poll_id().unwrap(), expected);
        let mono = BasicMonoUlidGenerator::<ULID, _, _>::new(&clock, MaskOnlyRand);
        assert_eq!(mono.try_poll_id().unwrap(), expected);
    }
    #[cfg(feature = "lock")]
    {
        let lock = LockMonoUlidGenerator::<ULID, _, _>::new(&clock, MaskOnlyRand);
        assert_eq!(lock.try_poll_id().unwrap(), expected);
    }
    #[cfg(all(feature = "atomic", target_has_atomic = "128"))]
    {
        let atomic = AtomicMonoUlidGenerator::<ULID, _, _>::new(&clock, MaskOnlyRand);
        assert_eq!(atomic.try_poll_id().unwrap(), expected);
    }
}
