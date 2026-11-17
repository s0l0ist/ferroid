use core::{cell::Cell, future::poll_fn, num::NonZeroU64, pin::pin, task::Poll, time::Duration};

use futures::FutureExt;

use crate::time::TimeSource;

struct TestClock {
    ticks: Cell<u64>,
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

#[cfg(feature = "snowflake")]
mod snowflake {
    use super::*;
    use crate::{
        futures::SnowflakeGeneratorAsyncExt, generator::BasicSnowflakeGenerator,
        id::SnowflakeTwitterId,
    };

    #[test]
    fn ready_returns_an_id() {
        let clock = TestClock {
            ticks: Cell::new(42),
        };
        let generator = BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, &clock);

        let id = generator
            .next_id_async(|_| async { panic!("unexpected backoff") })
            .now_or_never()
            .expect("generator should be ready");

        assert_eq!((id.timestamp(), id.sequence()), (42, 0));
    }

    #[test]
    fn pending_yields_then_returns_an_id() {
        let clock = TestClock {
            ticks: Cell::new(0),
        };
        let generator = BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(
            0,
            0,
            SnowflakeTwitterId::max_sequence(),
            &clock,
        );

        let mut future = pin!(generator.next_id_async(|delay| {
            assert_eq!(delay, Duration::from_millis(10));
            poll_fn(|_| {
                if clock.ticks.get() == 0 {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        }));

        assert!(future.as_mut().now_or_never().is_none());
        clock.ticks.set(1);
        let id = future.now_or_never().expect("generator should be ready");

        assert_eq!((id.timestamp(), id.sequence()), (1, 0));
    }

    #[test]
    fn clock_rollback_passes_scaled_duration() {
        let clock = TestClock {
            ticks: Cell::new(39),
        };
        let generator =
            BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(42, 0, 0, &clock);

        let id = generator
            .try_next_id_async(|delay| {
                assert_eq!(delay, Duration::from_millis(30));
                async { clock.ticks.set(43) }
            })
            .now_or_never()
            .expect("backoff should advance the clock")
            .unwrap();

        assert_eq!((id.timestamp(), id.sequence()), (43, 0));
    }
}

#[cfg(feature = "ulid")]
mod ulid {
    use super::*;
    use crate::{futures::UlidGeneratorAsyncExt, generator::BasicMonoUlidGenerator, id::ULID};

    struct ZeroRand;
    impl crate::rand::RandSource<u128> for ZeroRand {
        fn rand(&self) -> u128 {
            0
        }
    }

    #[test]
    fn ready_returns_an_id() {
        let clock = TestClock {
            ticks: Cell::new(42),
        };
        let generator = BasicMonoUlidGenerator::<ULID, _, _>::new(&clock, ZeroRand);

        let id = generator
            .next_id_async(|_| async { panic!("unexpected backoff") })
            .now_or_never()
            .expect("generator should be ready");

        assert_eq!((id.timestamp(), id.random()), (42, 0));
    }

    #[test]
    fn pending_yields_then_returns_an_id() {
        let clock = TestClock {
            ticks: Cell::new(0),
        };
        let generator = BasicMonoUlidGenerator::<ULID, _, _>::from_components(
            0,
            ULID::max_random(),
            &clock,
            ZeroRand,
        );

        let mut future = pin!(generator.next_id_async(|delay| {
            assert_eq!(delay, Duration::from_millis(10));
            poll_fn(|_| {
                if clock.ticks.get() == 0 {
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
        }));

        assert!(future.as_mut().now_or_never().is_none());
        clock.ticks.set(1);
        let id = future.now_or_never().expect("generator should be ready");

        assert_eq!((id.timestamp(), id.random()), (1, 0));
    }

    #[test]
    fn clock_rollback_passes_scaled_duration() {
        let clock = TestClock {
            ticks: Cell::new(39),
        };
        let generator =
            BasicMonoUlidGenerator::<ULID, _, _>::from_components(42, 0, &clock, ZeroRand);

        let id = generator
            .try_next_id_async(|delay| {
                assert_eq!(delay, Duration::from_millis(30));
                async { clock.ticks.set(43) }
            })
            .now_or_never()
            .expect("backoff should advance the clock")
            .unwrap();

        assert_eq!((id.timestamp(), id.random()), (43, 0));
    }
}
