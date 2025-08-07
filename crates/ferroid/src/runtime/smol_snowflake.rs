use crate::{Result, SmolSleep, SnowflakeGenerator, SnowflakeId, TimeSource};
use core::future::Future;

/// Extension trait for asynchronously generating Snowflake IDs using the
/// [`smol`](https://docs.rs/smol) async runtime.
///
/// This trait provides a convenience method for using a [`SleepProvider`]
/// backed by the `smol` runtime, allowing you to call `.try_next_id_async()`
/// without needing to specify the sleep strategy manually.
///
/// [`SleepProvider`]: crate::SleepProvider
pub trait SnowflakeGeneratorAsyncSmolExt<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err;
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
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID, Self::Err>>;
}

impl<G, ID, T> SnowflakeGeneratorAsyncSmolExt<ID, T> for G
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err = G::Err;

    #[allow(clippy::future_not_send)]
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID, Self::Err>> {
        <Self as crate::SnowflakeGeneratorAsyncExt<ID, T>>::try_next_id_async::<SmolSleep>(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AtomicSnowflakeGenerator, LockSnowflakeGenerator, MonotonicClock, Result, SleepProvider,
        SmolYield, SnowflakeGenerator, SnowflakeId, SnowflakeTwitterId, TimeSource,
    };
    use core::fmt;
    use futures::future::try_join_all;
    use smol::Task;
    use std::collections::HashSet;
    use std::vec::Vec;

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 8;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 8;

    #[test]
    fn generates_many_unique_ids_lock_smol_sleep() {
        smol::block_on(async {
            test_many_snow_unique_ids_explicit::<SnowflakeTwitterId, _, _, SmolSleep>(
                LockSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();
        });
    }
    #[test]
    fn generates_many_unique_ids_lock_smol_yield() {
        smol::block_on(async {
            test_many_snow_unique_ids_explicit::<SnowflakeTwitterId, _, _, SmolYield>(
                LockSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();
        });
    }
    #[test]
    fn generates_many_unique_ids_lock_smol_convenience() {
        smol::block_on(async {
            test_many_snow_unique_ids_convenience::<SnowflakeTwitterId, _, _>(
                LockSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();
        });
    }

    #[test]
    fn generates_many_unique_ids_atomic_smol_sleep() {
        smol::block_on(async {
            test_many_snow_unique_ids_explicit::<SnowflakeTwitterId, _, _, SmolSleep>(
                AtomicSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();
        });
    }
    #[test]
    fn generates_many_unique_ids_atomic_smol_yield() {
        smol::block_on(async {
            test_many_snow_unique_ids_explicit::<SnowflakeTwitterId, _, _, SmolYield>(
                AtomicSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();
        });
    }
    #[test]
    fn generates_many_unique_ids_atomic_smol_convenience() {
        smol::block_on(async {
            test_many_snow_unique_ids_convenience::<SnowflakeTwitterId, _, _>(
                AtomicSnowflakeGenerator::new,
                MonotonicClock::default,
            )
            .await
            .unwrap();
        });
    }

    // Helper function for explicit SleepProvider testing
    async fn test_many_snow_unique_ids_explicit<ID, G, T, S>(
        generator_fn: impl Fn(u64, T) -> G,
        clock_factory: impl Fn() -> T,
    ) -> Result<()>
    where
        G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
        ID: SnowflakeId + fmt::Debug + Send + 'static,
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
                        let id = crate::SnowflakeGeneratorAsyncExt::try_next_id_async::<S>(&g)
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
        clock_factory: impl Fn() -> T,
    ) -> Result<()>
    where
        G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
        ID: SnowflakeId + fmt::Debug + Send + 'static,
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
        tasks: Vec<Task<Result<Vec<impl SnowflakeId + fmt::Debug>>>>,
    ) -> Result<()> {
        let all_ids: Vec<_> = try_join_all(tasks).await?.into_iter().flatten().collect();

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
