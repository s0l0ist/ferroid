use crate::rand::RandSource;
use crate::time::TimeSource;
use core::{cell::Cell, num::NonZeroU64};

pub(super) struct CountingRand {
    next: Cell<u128>,
}

impl CountingRand {
    pub(super) fn new() -> Self {
        Self { next: Cell::new(0) }
    }
}

impl RandSource<u128> for CountingRand {
    fn rand(&self) -> u128 {
        let next = self.next.get();
        self.next.set(next + 1);
        next
    }
}

pub(super) struct ZeroRand;
impl RandSource<u128> for ZeroRand {
    fn rand(&self) -> u128 {
        0
    }
}

#[cfg(any(feature = "basic", feature = "lock", feature = "std"))]
pub(super) struct MockTime {
    pub(super) millis: u64,
}

#[cfg(any(feature = "basic", feature = "lock", feature = "std"))]
impl TimeSource<u128> for MockTime {
    fn current_millis(&self) -> u128 {
        u128::from(self.millis)
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

impl TimeSource<u128> for &TestClock {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(10).unwrap();

    fn current_millis(&self) -> u128 {
        u128::from(self.ticks.get())
    }
}

pub(super) struct OverflowClock;
impl TimeSource<u128> for OverflowClock {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::MAX;

    fn current_millis(&self) -> u128 {
        0
    }
}
