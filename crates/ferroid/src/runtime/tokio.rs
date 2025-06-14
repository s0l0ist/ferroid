use crate::{Result, SleepProvider, Snowflake, SnowflakeGenerator, TimeSource};
use core::pin::Pin;

/// Extension trait for asynchronously generating Snowflake IDs using the
/// [`tokio`](https://docs.rs/tokio) async runtime.
///
/// This trait provides a convenience method for using a [`SleepProvider`]
/// backed by the `tokio` runtime, allowing you to call `.try_next_id_async()`
/// without specifying the sleep strategy manually.
pub trait SnowflakeGeneratorAsyncTokioExt<ID, T>
where
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    /// Returns a future that resolves to the next available Snowflake ID using
    /// the [`TokioSleep`] provider.
    ///
    /// Internally delegates to
    /// [`SnowflakeGeneratorAsyncExt::try_next_id_async`] method with
    /// [`TokioSleep`] as the sleep strategy.
    ///
    /// # Errors
    ///
    /// This future may return an error if the underlying generator does.
    ///
    /// [`SnowflakeGeneratorAsyncExt::try_next_id_async`]:
    ///     crate::SnowflakeGeneratorAsyncExt::try_next_id_async
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID>>;
}

impl<G, ID, T> SnowflakeGeneratorAsyncTokioExt<ID, T> for G
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID>> {
        <Self as crate::SnowflakeGeneratorAsyncExt<ID, T>>::try_next_id_async::<TokioSleep>(self)
    }
}

/// An implementation of [`SleepProvider`] using Tokio's timer.
///
/// This is the default provider for use in async applications built on Tokio.
pub struct TokioSleep;
impl SleepProvider for TokioSleep {
    type Sleep = tokio::time::Sleep;

    fn sleep_for(dur: core::time::Duration) -> Self::Sleep {
        tokio::time::sleep(dur)
    }
}

/// An implementation of [`SleepProvider`] using Tokio's yield.
///
/// This strategy avoids timer-based delays by yielding to the scheduler
/// immediately, which can improve responsiveness in low-concurrency scenarios.
///
/// However, it comes at the cost of more frequent rescheduling, which can
/// result in tighter polling loops and increased CPU usage under load. In
/// highly concurrent cases, a timer-based sleep (e.g., [`TokioSleep`]) is often
/// more efficient due to reduced scheduler churn.
pub struct TokioYield;
impl SleepProvider for TokioYield {
    /// Tokio's `yield_now()` returns a private future type, so we must use a
    /// boxed `dyn Future` to abstract over it.
    type Sleep = Pin<Box<dyn Future<Output = ()> + Send>>;

    fn sleep_for(_dur: core::time::Duration) -> Self::Sleep {
        Box::pin(tokio::task::yield_now())
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
    use std::collections::HashSet;

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 32;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 32; // Enough to simulate at least 32 Pending cycles

    // Test the explicit SleepProvider approach
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_explicit_lock() -> Result<()> {
        test_many_unique_ids_explicit::<_, SnowflakeTwitterId, MonotonicClock, TokioSleep>(
            LockSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await?;
        test_many_unique_ids_explicit::<_, SnowflakeTwitterId, MonotonicClock, TokioYield>(
            LockSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_explicit_atomic() -> Result<()> {
        test_many_unique_ids_explicit::<_, SnowflakeTwitterId, MonotonicClock, TokioSleep>(
            AtomicSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await?;
        test_many_unique_ids_explicit::<_, SnowflakeTwitterId, MonotonicClock, TokioYield>(
            AtomicSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await
    }

    // Test the convenience extension trait approach
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_convenience_lock() -> Result<()> {
        test_many_unique_ids_convenience::<_, SnowflakeTwitterId, MonotonicClock>(
            LockSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_convenience_atomic() -> Result<()> {
        test_many_unique_ids_convenience::<_, SnowflakeTwitterId, MonotonicClock>(
            AtomicSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await
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
        let tasks: Vec<tokio::task::JoinHandle<Result<_>>> = generators
            .into_iter()
            .map(|g| {
                tokio::spawn(async move {
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

        // Test convenience extension trait syntax (uses TokioSleep by default)
        let tasks: Vec<tokio::task::JoinHandle<Result<_>>> = generators
            .into_iter()
            .map(|g| {
                tokio::spawn(async move {
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
        tasks: Vec<tokio::task::JoinHandle<Result<Vec<impl Snowflake + fmt::Debug>>>>,
    ) -> Result<()> {
        let all_ids: Vec<_> = try_join_all(tasks)
            .await
            .unwrap()
            .into_iter()
            .flat_map(Result::unwrap)
            .collect();

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
