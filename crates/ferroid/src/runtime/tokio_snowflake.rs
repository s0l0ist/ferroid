use crate::{Result, Snowflake, SnowflakeGenerator, TimeSource, TokioSleep};

/// Extension trait for asynchronously generating Snowflake IDs using the
/// [`tokio`](https://docs.rs/tokio) async runtime.
///
/// This trait provides a convenience method for using a [`SleepProvider`]
/// backed by the `tokio` runtime, allowing you to call `.try_next_id_async()`
/// without specifying the sleep strategy manually.
///
/// [`SleepProvider`]: crate::SleepProvider
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AtomicSnowflakeGenerator, LockSnowflakeGenerator, MonotonicClock, Result, SleepProvider,
        Snowflake, SnowflakeGenerator, SnowflakeTwitterId, TimeSource, TokioYield,
    };
    use core::fmt;
    use futures::future::try_join_all;
    use std::collections::HashSet;
    use std::vec::Vec;

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 8;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 8; // Enough to simulate at least 8 Pending cycles

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_lock_sleep() -> Result<()> {
        test_many_snow_unique_ids_explicit::<SnowflakeTwitterId, _, _, TokioSleep>(
            LockSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await?;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_lock_yield() -> Result<()> {
        test_many_snow_unique_ids_explicit::<SnowflakeTwitterId, _, _, TokioYield>(
            LockSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await?;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_lock_convenience() -> Result<()> {
        test_many_snow_unique_ids_convenience::<SnowflakeTwitterId, _, _>(
            LockSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_atomic_sleep() -> Result<()> {
        test_many_snow_unique_ids_explicit::<SnowflakeTwitterId, _, _, TokioSleep>(
            AtomicSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await?;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_atomic_yield() -> Result<()> {
        test_many_snow_unique_ids_explicit::<SnowflakeTwitterId, _, _, TokioYield>(
            AtomicSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await?;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_atomic_convenience() -> Result<()> {
        test_many_snow_unique_ids_convenience::<SnowflakeTwitterId, _, _>(
            AtomicSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await?;
        Ok(())
    }

    // Helper function for explicit SleepProvider testing
    async fn test_many_snow_unique_ids_explicit<ID, G, T, S>(
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

        validate_unique_snow_ids(tasks).await
    }

    // Helper function for convenience extension trait testing
    async fn test_many_snow_unique_ids_convenience<ID, G, T>(
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

        validate_unique_snow_ids(tasks).await
    }

    // Helper to validate uniqueness - shared between test approaches
    async fn validate_unique_snow_ids(
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
            assert!(seen.insert(id), "Duplicate ID found: {id:?}");
        }

        Ok(())
    }
}
