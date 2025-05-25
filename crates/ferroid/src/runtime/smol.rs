use crate::SleepProvider;
use pin_project_lite::pin_project;
use smol::Timer;
use std::{
    pin::Pin,
    task::{Context, Poll},
};

/// An implementation of [`SleepProvider`] using Smol's timer.
///
/// This is the default provider for use in async applications built on Smol.
pub struct SmolSleep;
impl SleepProvider for SmolSleep {
    type Sleep = SmolSleepFuture;

    fn sleep_for(dur: std::time::Duration) -> Self::Sleep {
        SmolSleepFuture {
            timer: Timer::after(dur),
        }
    }
}

pin_project! {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AtomicSnowflakeGenerator, LockSnowflakeGenerator, MonotonicClock, Result, Snowflake,
        SnowflakeGenerator, SnowflakeGeneratorAsyncExt, SnowflakeTwitterId, TimeSource,
    };
    use core::fmt;
    use futures::future::try_join_all;
    use smol::Task;
    use std::collections::HashSet;

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 32;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 32;

    #[test]
    fn generates_many_unique_ids_lock_smol() {
        smol::block_on(
            test_many_unique_ids::<_, SnowflakeTwitterId, MonotonicClock>(
                LockSnowflakeGenerator::new,
                MonotonicClock::default,
            ),
        )
        .unwrap();
    }

    #[test]
    fn generates_many_unique_ids_atomic_smol() {
        smol::block_on(
            test_many_unique_ids::<_, SnowflakeTwitterId, MonotonicClock>(
                AtomicSnowflakeGenerator::new,
                MonotonicClock::default,
            ),
        )
        .unwrap();
    }

    async fn test_many_unique_ids<G, ID, T>(
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

        let tasks: Vec<Task<Result<Vec<ID>>>> = generators
            .into_iter()
            .map(|g| {
                smol::spawn(async move {
                    let mut ids = Vec::with_capacity(IDS_PER_GENERATOR);
                    for _ in 0..IDS_PER_GENERATOR {
                        let id = g.try_next_id_async::<SmolSleep>().await?;
                        ids.push(id);
                    }
                    Ok(ids)
                })
            })
            .collect();

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
