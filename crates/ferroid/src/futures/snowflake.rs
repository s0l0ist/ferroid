use core::{convert::Infallible, future::Future, time::Duration};

use crate::{
    generator::{Poll, Result, SnowflakeGenerator},
    id::SnowflakeId,
    time::TimeSource,
};

/// Extension trait for asynchronous Snowflake generation.
///
/// Uses a caller-supplied async backoff strategy when generation is pending.
///
/// Local generators and callbacks are supported; the returned future
/// is `Send` when its captures and the callback's future are `Send`.
pub trait SnowflakeGeneratorAsyncExt<ID, T>: SnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    /// Waits for the next ID from an infallible generator.
    ///
    /// `f` receives a suggested [`Duration`], including [`Duration::ZERO`],
    /// with clock granularity already applied. It may sleep, yield, or perform
    /// any other async work. The generator awaits it before retrying. The
    /// delay is advisory; waits saturate at `u64::MAX`
    /// milliseconds.
    fn next_id_async<B, F>(&self, f: B) -> impl Future<Output = ID>
    where
        B: FnMut(Duration) -> F,
        F: Future<Output = ()>,
        Self::Err: Into<Infallible>;

    /// Waits for the next ID, propagating generator errors immediately.
    ///
    /// On [`Poll::Pending`], awaits `f` with the suggested [`Duration`]
    /// before polling again. The callback may perform any async work and may
    /// ignore the delay.
    /// Cancelling this future stops retrying; it does not reserve an ID.
    ///
    /// # Errors
    ///
    /// Returns the underlying generator error, such as mutex poisoning.
    fn try_next_id_async<B, F>(&self, f: B) -> impl Future<Output = Result<ID, Self::Err>>
    where
        B: FnMut(Duration) -> F,
        F: Future<Output = ()>;
}

impl<G, ID, T> SnowflakeGeneratorAsyncExt<ID, T> for G
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    async fn next_id_async<B, F>(&self, f: B) -> ID
    where
        B: FnMut(Duration) -> F,
        F: Future<Output = ()>,
        Self::Err: Into<Infallible>,
    {
        let Ok(id) = self
            .try_next_id_async(f)
            .await
            .map_err(Into::<Infallible>::into);
        id
    }

    async fn try_next_id_async<B, F>(&self, mut f: B) -> Result<ID, Self::Err>
    where
        B: FnMut(Duration) -> F,
        F: Future<Output = ()>,
    {
        loop {
            let delay = match self.try_poll_id()? {
                Poll::Ready { id } => return Ok(id),
                Poll::Pending { yield_for } => yield_for,
            };
            f(delay).await;
        }
    }
}

#[cfg(all(test, feature = "basic"))]
mod tests {
    use core::{cell::Cell, future::poll_fn, pin::pin, task::Poll, time::Duration};

    use futures::FutureExt;

    use crate::futures::tests::TestClock;
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

#[cfg(all(test, feature = "lock", not(feature = "parking-lot")))]
mod lock_tests {
    use crate::{
        futures::SnowflakeGeneratorAsyncExt, generator::LockSnowflakeGenerator,
        id::SnowflakeTwitterId, time::DirectClock,
    };
    use futures::FutureExt;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn poisoned_lock_returns_error_without_backoff() {
        let generator =
            LockSnowflakeGenerator::<SnowflakeTwitterId, _>::new(0, DirectClock::default());
        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _guard = generator.state.lock();
            panic!("poison the mutex");
        }));
        assert!(panic.is_err());

        let error = generator
            .try_next_id_async(|_| async { panic!("unexpected backoff") })
            .now_or_never()
            .expect("generator errors must not suspend")
            .unwrap_err();
        assert!(matches!(error, crate::generator::Error::LockPoisoned));
    }
}
