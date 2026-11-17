use crate::time::TimeSource;
use core::{cell::Cell, num::NonZeroU64};

pub(super) struct TestClock {
    pub(super) ticks: Cell<u64>,
}

impl TimeSource<u64> for &TestClock {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(10).unwrap();

    fn current_millis(&self) -> u64 {
        self.ticks.get()
    }
}

impl TimeSource<u128> for &TestClock {
    const GRANULARITY_MILLIS: NonZeroU64 = NonZeroU64::new(10).unwrap();

    fn current_millis(&self) -> u128 {
        u128::from(self.ticks.get())
    }
}
