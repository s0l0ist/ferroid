use crate::{Result, SleepProvider, Snowflake, SnowflakeGenerator, TimeSource};
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use pin_project_lite::pin_project;
use smol::Timer;

/// Extension trait for asynchronously generating Snowflake IDs using the
/// [`smol`](https://docs.rs/smol) async runtime.
///
/// This trait provides a convenience method for using a [`SleepProvider`]
/// backed by the `smol` runtime, allowing you to call `.try_next_id_async()`
/// without needing to specify the sleep strategy manually.
pub trait SnowflakeGeneratorAsyncSmolExt<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    /// Returns a future that resolves to the next available Snowflake ID using
    /// the [`SmolSleep`] provider.
    ///
    /// Internally delegates to
    /// [`SnowflakeGeneratorAsyncExt::try_next_id_async`] method with
    /// [`SmolSleep`] as the sleep strategy.
    ///
    /// # Errors
    ///
    /// This future may return an error if the underlying generator does.
    ///
    /// [`SnowflakeGeneratorAsyncExt::try_next_id_async`]:
    ///     crate::SnowflakeGeneratorAsyncExt::try_next_id_async
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID>>;
}

impl<G, ID, T> SnowflakeGeneratorAsyncSmolExt<ID, T> for G
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID>> {
        <Self as crate::SnowflakeGeneratorAsyncExt<ID, T>>::try_next_id_async::<SmolSleep>(self)
    }
}

/// An implementation of [`SleepProvider`] using Smol's timer.
///
/// This is the default provider for use in async applications built on Smol.
pub struct SmolSleep;
impl SleepProvider for SmolSleep {
    type Sleep = SmolSleepFuture;

    fn sleep_for(dur: core::time::Duration) -> Self::Sleep {
        SmolSleepFuture {
            timer: Timer::after(dur),
        }
    }
}

pin_project! {
    /// Internal future returned by [`SmolSleep::sleep_for`].
    ///
    /// This type wraps a [`smol::Timer`] and implements [`Future`] with `Output
    /// = ()`, discarding the timer's `Instant` result.
    ///
    /// You should not construct or use this type directly. It is only used
    /// internally by the [`SleepProvider`] implementation for the Smol runtime.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct SmolSleepFuture {
        #[pin]
        timer: Timer,
    }
}

impl Future for SmolSleepFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        match self.project().timer.poll(cx) {
            Poll::Ready(_) => Poll::Ready(()),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// An implementation of [`SleepProvider`] using Smol's yield.
///
/// This strategy avoids timer-based delays by yielding to the scheduler
/// immediately, which can improve responsiveness in low-concurrency scenarios.
///
/// However, it comes at the cost of more frequent rescheduling, which can
/// result in tighter polling loops and increased CPU usage under load. In
/// highly concurrent cases, a timer-based sleep (e.g., [`SmolSleep`]) is often
/// more efficient due to reduced scheduler churn.
pub struct SmolYield;
impl SleepProvider for SmolYield {
    type Sleep = smol::future::YieldNow;

    fn sleep_for(_dur: core::time::Duration) -> Self::Sleep {
        smol::future::yield_now()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AtomicSnowflakeGenerator, LockSnowflakeGenerator, MonotonicClock, Result, Snowflake,
        SnowflakeGenerator, SnowflakeTwitterId, TimeSource,
    };
    use core::fmt;
    use futures::future::try_join_all;
    use smol::Task;
    use std::collections::HashSet;

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 32;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 32;

    // Test the explicit SleepProvider approach
    #[test]
    fn generates_many_unique_ids_explicit_lock_smol() {
        smol::block_on(async {
            test_many_unique_ids_explicit::<_, SnowflakeTwitterId, MonotonicClock, SmolSleep>(
                LockSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();

            test_many_unique_ids_explicit::<_, SnowflakeTwitterId, MonotonicClock, SmolYield>(
                LockSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();
        });
    }

    #[test]
    fn generates_many_unique_ids_explicit_atomic_smol() {
        smol::block_on(async {
            test_many_unique_ids_explicit::<_, SnowflakeTwitterId, MonotonicClock, SmolSleep>(
                AtomicSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();

            test_many_unique_ids_explicit::<_, SnowflakeTwitterId, MonotonicClock, SmolYield>(
                AtomicSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();
        });
    }

    // Test the convenience extension trait approach
    #[test]
    fn generates_many_unique_ids_convenience_lock_smol() {
        smol::block_on(test_many_unique_ids_convenience::<
            _,
            SnowflakeTwitterId,
            MonotonicClock,
        >(
            LockSnowflakeGenerator::new, MonotonicClock::default
        ))
        .unwrap();
    }

    #[test]
    fn generates_many_unique_ids_convenience_atomic_smol() {
        smol::block_on(test_many_unique_ids_convenience::<
            _,
            SnowflakeTwitterId,
            MonotonicClock,
        >(
            AtomicSnowflakeGenerator::new, MonotonicClock::default
        ))
        .unwrap();
    }

    // Helper function for explicit SleepProvider testing
    async fn test_many_unique_ids_explicit<G, ID, T, S>(
        generator_fn: impl Fn(u64, T) -> G,
        clock_factory: impl Fn() -> T,
    ) -> Result<()>
    where
        G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
        ID: Snowflake + fmt::Debug + Send + 'static,
        T: TimeSource<ID::Ty> + Clone + Send,
        S: SleepProvider,
    {
        let clock = clock_factory();
        let generators: Vec<_> = (0..NUM_GENERATORS)
            .map(|machine_id| generator_fn(machine_id, clock.clone()))
            .collect();

        // Test explicit SleepProvider syntax
        let tasks: Vec<Task<Result<Vec<ID>>>> = generators
            .into_iter()
            .map(|g| {
                smol::spawn(async move {
                    let mut ids = Vec::with_capacity(IDS_PER_GENERATOR);
                    for _ in 0..IDS_PER_GENERATOR {
                        let id =
                            crate::SnowflakeGeneratorAsyncExt::try_next_id_async::<S>(&g).await?;
                        ids.push(id);
                    }
                    Ok(ids)
                })
            })
            .collect();

        validate_unique_ids(tasks).await
    }

    // Helper function for convenience extension trait testing
    async fn test_many_unique_ids_convenience<G, ID, T>(
        generator_fn: impl Fn(u64, T) -> G,
        clock_factory: impl Fn() -> T,
    ) -> Result<()>
    where
        G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
        ID: Snowflake + fmt::Debug + Send + 'static,
        T: TimeSource<ID::Ty> + Clone + Send,
    {
        let clock = clock_factory();
        let generators: Vec<_> = (0..NUM_GENERATORS)
            .map(|machine_id| generator_fn(machine_id, clock.clone()))
            .collect();

        // Test convenience extension trait syntax (uses SmolSleep by default)
        let tasks: Vec<Task<Result<Vec<ID>>>> = generators
            .into_iter()
            .map(|g| {
                smol::spawn(async move {
                    let mut ids = Vec::with_capacity(IDS_PER_GENERATOR);
                    for _ in 0..IDS_PER_GENERATOR {
                        // This uses the convenience method - no explicit
                        // SleepProvider type!
                        let id = g.try_next_id_async().await?;
                        ids.push(id);
                    }
                    Ok(ids)
                })
            })
            .collect();

        validate_unique_ids(tasks).await
    }

    // Helper to validate uniqueness - shared between test approaches
    async fn validate_unique_ids(
        tasks: Vec<Task<Result<Vec<impl Snowflake + fmt::Debug>>>>,
    ) -> Result<()> {
        let all_ids: Vec<_> = try_join_all(tasks).await?.into_iter().flatten().collect();

        let expected_total = NUM_GENERATORS as usize * IDS_PER_GENERATOR;
        assert_eq!(
            all_ids.len(),
            expected_total,
            "Expected {} IDs but got {}",
            expected_total,
            all_ids.len()
        );

        let mut seen = HashSet::with_capacity(all_ids.len());
        for id in &all_ids {
            assert!(seen.insert(id), "Duplicate ID found: {:?}", id);
        }

        Ok(())
    }
}
