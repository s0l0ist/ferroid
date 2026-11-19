use crate::{rand::RandSource, time::TimeSource};
use core::{cell::Cell, num::NonZeroU64};

pub(super) struct CountingRand {
    next: Cell<u64>,
}

impl CountingRand {
    pub(super) fn new() -> Self {
        Self { next: Cell::new(0) }
    }
}

impl<T: From<u64>> RandSource<T> for CountingRand {
    fn rand(&self) -> T {
        let next = self.next.get();
        self.next.set(next + 1);
        T::from(next)
    }
}

pub(super) struct ZeroRand;
impl<T: From<u64>> RandSource<T> for ZeroRand {
    fn rand(&self) -> T {
        T::from(0)
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
