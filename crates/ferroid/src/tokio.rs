use crate::SleepProvider;

/// An implementation of [`SleepProvider`] using Tokio’s timer.
///
/// This is the default provider for use in async applications built on Tokio.
pub struct TokioSleep;
impl SleepProvider for TokioSleep {
    type Sleep = tokio::time::Sleep;

    fn sleep_for(dur: tokio::time::Duration) -> Self::Sleep {
        tokio::time::sleep(dur)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AtomicSnowflakeGenerator, LockSnowflakeGenerator, MonotonicClock, Result, Snowflake,
        SnowflakeGenerator, SnowflakeGeneratorAsyncExt, SnowflakeTwitterId, TimeSource,
    };
    use core::fmt;
    use futures::future::try_join_all;
    use std::collections::HashSet;

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 32;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 32; // Enough to simulate at least 32 Pending cycles

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_lock() -> Result<()> {
        test_many_unique_ids::<_, SnowflakeTwitterId, MonotonicClock>(
            LockSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_atomic() -> Result<()> {
        test_many_unique_ids::<_, SnowflakeTwitterId, MonotonicClock>(
            AtomicSnowflakeGenerator::new,
            MonotonicClock::default,
        )
        .await
    }

    async fn test_many_unique_ids<G, ID, T>(
        generator_fn: impl Fn(u64, T) -> G,
        clock_factory: impl Fn() -> T,
    ) -> Result<()>
    where
        G: SnowflakeGenerator<ID, T> + Send + Sync + 'static,
        ID: Snowflake + Send + fmt::Debug + 'static,
        T: TimeSource<ID::Ty> + Clone + Send,
    {
        let clock = clock_factory();
        let generators: Vec<_> = (0..NUM_GENERATORS)
            .map(|machine_id| generator_fn(machine_id, clock.clone()))
            .collect();

        // Spawn one future per generator, each producing N IDs
        let tasks: Vec<tokio::task::JoinHandle<Result<_>>> = generators
            .into_iter()
            .map(|g| {
                tokio::spawn(async move {
                    let mut ids = Vec::with_capacity(IDS_PER_GENERATOR);
                    for _ in 0..IDS_PER_GENERATOR {
                        let id = g.try_next_id_async::<TokioSleep>().await?;
                        ids.push(id);
                    }
                    Ok(ids)
                })
            })
            .collect();

        let all_ids: Vec<_> = try_join_all(tasks)
            .await?
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
