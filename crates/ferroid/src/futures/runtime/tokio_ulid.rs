use crate::{
    RandSource, Result, TimeSource, futures::TokioSleep, generator::UlidGenerator, id::UlidId,
};
use core::future::Future;

/// Extension trait for asynchronously generating ULIDs using the
/// [`tokio`](https://docs.rs/tokio) async runtime.
///
/// This trait provides a convenience method for using a [`SleepProvider`]
/// backed by the `tokio` runtime, allowing you to call `.try_next_id_async()`
/// without specifying the sleep strategy manually.
///
/// [`SleepProvider`]: crate::futures::SleepProvider
pub trait UlidGeneratorAsyncTokioExt<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    type Err;

    /// Returns a future that resolves to the next available ULID using
    /// the [`TokioSleep`] provider.
    ///
    /// Internally delegates to
    /// [`UlidGeneratorAsyncExt::try_next_id_async`] method with
    /// [`TokioSleep`] as the sleep strategy.
    ///
    /// # Errors
    ///
    /// This future may return an error if the underlying generator does.
    ///
    /// [`UlidGeneratorAsyncExt::try_next_id_async`]:
    ///     crate::futures::UlidGeneratorAsyncExt::try_next_id_async
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID, Self::Err>>;
}

impl<G, ID, T, R> UlidGeneratorAsyncTokioExt<ID, T, R> for G
where
    G: UlidGenerator<ID, T, R> + Sync,
    ID: UlidId + Send,
    T: TimeSource<ID::Ty> + Send,
    R: RandSource<ID::Ty> + Send,
{
    type Err = G::Err;

    fn try_next_id_async(&self) -> impl Future<Output = Result<ID, Self::Err>> {
        <Self as crate::futures::UlidGeneratorAsyncExt<ID, T, R>>::try_next_id_async::<TokioSleep>(
            self,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        MonotonicClock, Result, ThreadRandom, TimeSource,
        futures::{SleepProvider, TokioYield},
        generator::LockMonoUlidGenerator,
        id::ULID,
    };
    use futures::future::try_join_all;
    use std::collections::HashSet;
    use std::vec::Vec;

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 8;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 8; // Enough to simulate at least 8 Pending cycles

    // Test the explicit SleepProvider approach
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_basic_sleep() -> Result<()> {
        test_many_ulid_unique_ids_explicit::<ULID, _, _, _, TokioSleep>(
            LockMonoUlidGenerator::new,
            MonotonicClock::default,
            ThreadRandom::default,
        )
        .await?;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_basic_yield() -> Result<()> {
        test_many_ulid_unique_ids_explicit::<ULID, _, _, _, TokioYield>(
            LockMonoUlidGenerator::new,
            MonotonicClock::default,
            ThreadRandom::default,
        )
        .await?;
        Ok(())
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_basic_convience() -> Result<()> {
        test_many_ulid_unique_ids_convenience::<ULID, _, _, _>(
            LockMonoUlidGenerator::new,
            MonotonicClock::default,
            ThreadRandom::default,
        )
        .await?;
        Ok(())
    }

    // Helper function for explicit SleepProvider testing
    async fn test_many_ulid_unique_ids_explicit<ID, G, T, R, S>(
        generator_fn: impl Fn(T, R) -> G,
        clock_factory: impl Fn() -> T,
        rand_factory: impl Fn() -> R,
    ) -> Result<()>
    where
        G: UlidGenerator<ID, T, R> + Send + Sync + 'static,
        ID: UlidId + Send + 'static,
        T: TimeSource<ID::Ty> + Clone + Send,
        R: RandSource<ID::Ty> + Clone + Send,
        S: SleepProvider,
    {
        let clock = clock_factory();
        let rand = rand_factory();
        let generators: Vec<_> = (0..NUM_GENERATORS)
            .map(|_| generator_fn(clock.clone(), rand.clone()))
            .collect();

        // Test explicit SleepProvider syntax
        let tasks: Vec<tokio::task::JoinHandle<Result<_>>> = generators
            .into_iter()
            .map(|g| {
                tokio::spawn(async move {
                    let mut ids = Vec::with_capacity(IDS_PER_GENERATOR);
                    for _ in 0..IDS_PER_GENERATOR {
                        let id = crate::futures::UlidGeneratorAsyncExt::try_next_id_async::<S>(&g)
                            .await
                            .unwrap();
                        ids.push(id);
                    }
                    Ok(ids)
                })
            })
            .collect();

        validate_unique_ulid_ids(tasks).await
    }

    // Helper function for convenience extension trait testing
    async fn test_many_ulid_unique_ids_convenience<ID, G, T, R>(
        generator_fn: impl Fn(T, R) -> G,
        clock_factory: impl Fn() -> T,
        rand_factory: impl Fn() -> R,
    ) -> Result<()>
    where
        G: UlidGenerator<ID, T, R> + Send + Sync + 'static,
        ID: UlidId + Send + 'static,
        T: TimeSource<ID::Ty> + Clone + Send,
        R: RandSource<ID::Ty> + Clone + Send,
    {
        let clock = clock_factory();
        let rand = rand_factory();
        let generators: Vec<_> = (0..NUM_GENERATORS)
            .map(|_| generator_fn(clock.clone(), rand.clone()))
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

        validate_unique_ulid_ids(tasks).await
    }

    // Helper to validate uniqueness - shared between test approaches
    async fn validate_unique_ulid_ids(
        tasks: Vec<tokio::task::JoinHandle<Result<Vec<impl UlidId>>>>,
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
