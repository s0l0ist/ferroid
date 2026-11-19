use crate::time::TimeSource;
use core::{cell::Cell, num::NonZeroU64};

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

pub(super) struct TestClock<T> {
    pub(super) ticks: Cell<T>,
}

impl<T> TestClock<T> {
    pub(super) fn new(ticks: T) -> Self {
        Self {
            ticks: Cell::new(ticks),
        }
    }
}

impl<T: Copy> TimeSource<T> for &TestClock<T> {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(10).unwrap();

    fn current_millis(&self) -> T {
        self.ticks.get()
    }
}

pub(super) struct OverflowClock;
impl<T: From<u64>> TimeSource<T> for OverflowClock {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::MAX;

    fn current_millis(&self) -> T {
        T::from(0)
    }
}
