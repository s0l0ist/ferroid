use core::{convert::Infallible, future::Future};

use crate::{
    futures::TokioSleep,
    generator::{Result, SnowflakeGenerator},
    id::SnowflakeId,
    time::TimeSource,
};

/// Extension trait for asynchronously generating Snowflake IDs using the
/// [`tokio`](https://docs.rs/tokio) async runtime.
///
/// This trait provides convenience methods that use [`TokioSleep`] as the sleep
/// provider, allowing you to call async methods without manually specifying the
/// sleep strategy.
///
/// [`TokioSleep`]: crate::futures::TokioSleep
pub trait SnowflakeGeneratorAsyncTokioExt<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err;

    /// Returns a future that resolves to the next available Snowflake ID.
    ///
    /// This infallible method uses [`TokioSleep`] and is only available for
    /// generators with infallible error types.
    ///
    /// [`TokioSleep`]: crate::futures::TokioSleep
    fn next_id_async(&self) -> impl Future<Output = ID>
    where
        Self::Err: Into<Infallible>;

    /// Returns a future that resolves to the next available Snowflake ID using
    /// [`TokioSleep`].
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying generator fails.
    ///
    /// [`TokioSleep`]: crate::futures::TokioSleep
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID, Self::Err>>;
}

impl<G, ID, T> SnowflakeGeneratorAsyncTokioExt<ID, T> for G
where
    G: SnowflakeGenerator<ID, T> + Sync,
    ID: SnowflakeId + Send,
    T: TimeSource<ID::Ty> + Send,
{
    type Err = G::Err;

    fn next_id_async(&self) -> impl Future<Output = ID>
    where
        Self::Err: Into<Infallible>,
    {
        <Self as crate::futures::SnowflakeGeneratorAsyncExt<ID, T>>::next_id_async::<TokioSleep>(
            self,
        )
    }

    fn try_next_id_async(&self) -> impl Future<Output = Result<ID, Self::Err>> {
        <Self as crate::futures::SnowflakeGeneratorAsyncExt<ID, T>>::try_next_id_async::<TokioSleep>(
            self,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, vec::Vec};

    use futures::future::try_join_all;

    use super::*;
    use crate::{
        futures::{SleepProvider, TokioYield},
        generator::{AtomicSnowflakeGenerator, LockSnowflakeGenerator, Result, SnowflakeGenerator},
        id::{SnowflakeId, SnowflakeTwitterId},
        time::{MonotonicClock, TimeSource},
    };

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 8;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 8; // Enough to simulate at least 8 Pending cycles

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn atomic_can_call_next_id_async() {
        let generator = AtomicSnowflakeGenerator::new(0, MonotonicClock::default());
        let id = generator.next_id_async().await;
        assert!(matches!(id, SnowflakeTwitterId { .. }));

        let id = SnowflakeGeneratorAsyncTokioExt::next_id_async(&generator).await;
        assert!(matches!(id, SnowflakeTwitterId { .. }));
    }

    #[cfg(feature = "parking-lot")]
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn lock_can_call_next_id_async() {
        let generator = LockSnowflakeGenerator::new(0, MonotonicClock::default());
        let id = generator.next_id_async().await;
        assert!(matches!(id, SnowflakeTwitterId { .. }));

        let id = SnowflakeGeneratorAsyncTokioExt::next_id_async(&generator).await;
        assert!(matches!(id, SnowflakeTwitterId { .. }));
    }

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
        clock_fn: impl Fn() -> T,
    ) -> Result<()>
    where
        G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
        ID: SnowflakeId + Send + 'static,
        T: TimeSource<ID::Ty> + Clone + Send,
        S: SleepProvider,
    {
        let clock = clock_fn();
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
                            crate::futures::SnowflakeGeneratorAsyncExt::try_next_id_async::<S>(&g)
                                .await
                                .unwrap();
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
        clock_fn: impl Fn() -> T,
    ) -> Result<()>
    where
        G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
        ID: SnowflakeId + Send + 'static,
        T: TimeSource<ID::Ty> + Clone + Send,
    {
        let clock = clock_fn();
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
                        let id = g.try_next_id_async().await.unwrap();
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
        tasks: Vec<tokio::task::JoinHandle<Result<Vec<impl SnowflakeId>>>>,
    ) -> Result<()> {
        let all_ids: Vec<_> = try_join_all(tasks)
            .await
            .unwrap()
            .into_iter()
            .flat_map(Result::unwrap)
            .collect();

        #[allow(clippy::cast_possible_truncation)]
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
