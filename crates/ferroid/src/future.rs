use crate::{IdGenStatus, Result, SnowflakeGenerator, TimeSource, ToU64, id::Snowflake};
use pin_project_lite::pin_project;
use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

/// Extension trait for asynchronously generating Snowflake IDs.
///
/// This trait enables `SnowflakeGenerator` types to yield IDs in a
/// `Future`-based context by awaiting until the generator is ready to produce a
/// new ID.
///
/// The default implementation uses [`GeneratorFuture`] and a specified
/// [`SleepProvider`] to yield when the generator is not yet ready.
pub trait SnowflakeGeneratorAsyncExt<'a, ID, T> {
    /// Returns a future that resolves to the next available Snowflake ID.
    ///
    /// If the generator is not ready to issue a new ID immediately, the future
    /// will sleep for the amount of time indicated by the generator and retry.
    ///
    /// # Errors
    ///
    /// This future may return an error if the generator encounters one.
    fn try_next_id_async<S>(&'a mut self) -> impl Future<Output = Result<ID>> + 'a
    where
        Self: SnowflakeGenerator<ID, T>,
        ID: Snowflake + 'a,
        T: TimeSource<ID::Ty> + 'a,
        S: SleepProvider + 'a;
}

impl<'a, G, ID, T> SnowflakeGeneratorAsyncExt<'a, ID, T> for G {
    fn try_next_id_async<S>(&'a mut self) -> impl Future<Output = Result<ID>> + 'a
    where
        G: SnowflakeGenerator<ID, T>,
        ID: Snowflake + 'a,
        T: TimeSource<ID::Ty> + 'a,
        S: SleepProvider + 'a,
    {
        GeneratorFuture::<'a, G, ID, T, S>::new(self)
    }
}

/// A trait that abstracts over how to sleep for a given [`Duration`] in async
/// contexts.
///
/// This allows the generator to be generic over runtimes like `Tokio` or
/// `async-std`.
pub trait SleepProvider {
    type Sleep: Future<Output = ()>;

    fn sleep_for(dur: Duration) -> Self::Sleep;
}

/// An implementation of [`SleepProvider`] using Tokio’s timer.
///
/// This is the default provider for use in async applications built on Tokio.
pub struct TokioSleep;
impl SleepProvider for TokioSleep {
    type Sleep = tokio::time::Sleep;

    fn sleep_for(dur: Duration) -> Self::Sleep {
        tokio::time::sleep(dur)
    }
}

pin_project! {
    /// A future that polls a [`SnowflakeGenerator`] until it is ready to
    /// produce an ID.
    ///
    /// This future handles `Pending` responses by sleeping for a recommended
    /// amount of time before polling the generator again.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct GeneratorFuture<'a, G, ID, T, S>
    where
        G: SnowflakeGenerator<ID, T>,
        ID: Snowflake,
        T: TimeSource<ID::Ty>,
        S: SleepProvider,
    {
        generator: &'a mut G,
        #[pin]
        sleep: Option<S::Sleep>,
        _idt: PhantomData<(ID, T)>
    }
}

impl<'a, G, ID, T, S> GeneratorFuture<'a, G, ID, T, S>
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
    S: SleepProvider,
{
    /// Constructs a new [`GeneratorFuture`] from a given generator.
    ///
    /// This does not immediately begin polling the generator; instead, it will
    /// attempt to produce an ID when `.poll()` is called.
    pub fn new(generator: &'a mut G) -> Self {
        Self {
            generator,
            sleep: None,
            _idt: PhantomData,
        }
    }
}

impl<'a, G, ID, T, S> Future for GeneratorFuture<'a, G, ID, T, S>
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
    S: SleepProvider,
{
    type Output = Result<ID>;

    /// Polls the generator for a new ID.
    ///
    /// If the generator is not ready, this will register the task waker and
    /// sleep for the time recommended by the generator before polling again.
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();

        if let Some(sleep) = this.sleep.as_mut().as_pin_mut() {
            match sleep.poll(cx) {
                Poll::Pending => {
                    return Poll::Pending;
                }
                Poll::Ready(()) => {
                    this.sleep.set(None);
                }
            }
        };
        match this.generator.try_next_id()? {
            IdGenStatus::Ready { id } => Poll::Ready(Ok(id)),
            IdGenStatus::Pending { yield_for } => {
                let sleep_fut = S::sleep_for(Duration::from_millis(yield_for.to_u64()?));
                this.sleep.as_mut().set(Some(sleep_fut));
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generator::BasicSnowflakeGenerator, id::SnowflakeTwitterId, time::MonotonicClock};
    use futures::future::try_join_all;
    use std::collections::HashSet;

    const TOTAL_IDS: usize = 4096;
    const NUM_GENERATORS: u64 = 32;
    const IDS_PER_GENERATOR: usize = TOTAL_IDS * 32; // Enough to simulate at least 32 Pending cycles

    #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
    async fn generates_many_unique_ids_across_threads() -> Result<()> {
        let shared_clock = MonotonicClock::default();

        let generators: Vec<_> = (0..NUM_GENERATORS)
            .map(|machine_id| {
                BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(
                    machine_id,
                    shared_clock.clone(),
                )
            })
            .collect();

        // Spawn one future per generator, each producing N IDs
        let tasks: Vec<tokio::task::JoinHandle<Result<_>>> = generators
            .into_iter()
            .map(|mut g| {
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
            .map(Result::unwrap)
            .flatten()
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
            let inserted = seen.insert(id);
            assert!(inserted, "Duplicate ID found: {:?}", id);
        }

        Ok(())
    }
}
