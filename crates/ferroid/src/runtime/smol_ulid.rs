use crate::{RandSource, Result, SmolSleep, TimeSource, Ulid, UlidGenerator};

/// Extension trait for asynchronously generating ULIDs using the
/// [`smol`](https://docs.rs/smol) async runtime.
///
/// This trait provides a convenience method for using a [`SleepProvider`]
/// backed by the `smol` runtime, allowing you to call `.try_next_id_async()`
/// without needing to specify the sleep strategy manually.
///
/// [`SleepProvider`]: crate::SleepProvider
pub trait UlidGeneratorAsyncSmolExt<ID, T, R>
where
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    /// Returns a future that resolves to the next available Ulid using
    /// the [`SmolSleep`] provider.
    ///
    /// Internally delegates to
    /// [`UlidGeneratorAsyncExt::try_next_id_async`] method with
    /// [`SmolSleep`] as the sleep strategy.
    ///
    /// # Errors
    ///
    /// This future may return an error if the underlying generator does.
    ///
    /// [`UlidGeneratorAsyncExt::try_next_id_async`]:
    ///     crate::UlidGeneratorAsyncExt::try_next_id_async
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID>>;
}

impl<G, ID, T, R> UlidGeneratorAsyncSmolExt<ID, T, R> for G
where
    G: UlidGenerator<ID, T, R>,
    ID: Ulid,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    fn try_next_id_async(&self) -> impl Future<Output = Result<ID>> {
        <Self as crate::UlidGeneratorAsyncExt<ID, T, R>>::try_next_id_async::<SmolSleep>(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LockUlidGenerator, MonotonicClock, RandSource, Result, SleepProvider, SmolYield,
        ThreadRandom, TimeSource, ULID, Ulid, UlidGenerator,
    };
    use core::fmt;
    use futures::future::try_join_all;
    use smol::Task;
    use std::collections::HashSet;

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 8;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 8;

    // Test the explicit SleepProvider approach
    #[test]
    fn generates_many_unique_ids_basic_smol_sleep() {
        smol::block_on(async {
            test_many_ulid_unique_ids_explicit::<ULID, _, _, _, SmolSleep>(
                LockUlidGenerator::new,
                MonotonicClock::default,
                ThreadRandom::default,
            )
            .await
            .unwrap();
        });
    }
    #[test]
    fn generates_many_unique_ids_basic_smol_yield() {
        smol::block_on(async {
            test_many_ulid_unique_ids_explicit::<ULID, _, _, _, SmolYield>(
                LockUlidGenerator::new,
                MonotonicClock::default,
                ThreadRandom::default,
            )
            .await
            .unwrap();
        });
    }
    #[test]
    fn generates_many_unique_ids_basic_smol_convience() {
        smol::block_on(async {
            test_many_ulid_unique_ids_convenience::<ULID, _, _, _>(
                LockUlidGenerator::new,
                MonotonicClock::default,
                ThreadRandom::default,
            )
            .await
            .unwrap();
        });
    }

    // Helper function for explicit SleepProvider testing
    async fn test_many_ulid_unique_ids_explicit<ID, G, T, R, S>(
        generator_fn: impl Fn(T, R) -> G,
        clock_factory: impl Fn() -> T,
        rand_factory: impl Fn() -> R,
    ) -> Result<()>
    where
        G: UlidGenerator<ID, T, R> + Send + Sync + 'static,
        ID: Ulid + fmt::Debug + Send + 'static,
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
        let tasks: Vec<Task<Result<Vec<ID>>>> = generators
            .into_iter()
            .map(|g| {
                smol::spawn(async move {
                    let mut ids = Vec::with_capacity(IDS_PER_GENERATOR);
                    for _ in 0..IDS_PER_GENERATOR {
                        let id = crate::UlidGeneratorAsyncExt::try_next_id_async::<S>(&g).await?;
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
        ID: Ulid + fmt::Debug + Send + 'static,
        T: TimeSource<ID::Ty> + Clone + Send,
        R: RandSource<ID::Ty> + Clone + Send,
    {
        let clock = clock_factory();
        let rand = rand_factory();
        let generators: Vec<_> = (0..NUM_GENERATORS)
            .map(|_| generator_fn(clock.clone(), rand.clone()))
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

        validate_unique_ulid_ids(tasks).await
    }

    // Helper to validate uniqueness - shared between test approaches
    async fn validate_unique_ulid_ids(
        tasks: Vec<Task<Result<Vec<impl Ulid + fmt::Debug>>>>,
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
