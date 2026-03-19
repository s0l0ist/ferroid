use core::{convert::Infallible, future::Future, time::Duration};

use super::SleepProvider;
use crate::{
    generator::{Poll, Result, SnowflakeGenerator},
    id::{SnowflakeId, ToU64},
    time::TimeSource,
};

/// Extension trait for asynchronously generating Snowflake IDs.
///
/// This trait enables `SnowflakeGenerator` types to yield IDs in a
/// `Future`-based context by awaiting until the generator is ready to produce a
/// new ID.
pub trait SnowflakeGeneratorAsyncExt<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err;

    /// Returns a future that resolves to the next available ID.
    ///
    /// This infallible method automatically retries when the generator is
    /// temporarily unable to produce an ID. Only available for generators with
    /// infallible error types.
    ///
    /// For fallible generators, use
    /// [`Self::try_next_id_async`]
    fn next_id_async<S>(&self) -> impl Future<Output = ID>
    where
        S: SleepProvider,
        Self::Err: Into<Infallible>;

    /// Returns a future that resolves to the next available ID.
    ///
    /// Automatically retries when the generator is temporarily unable to
    /// produce an ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the generator fails, such as from lock poisoning.
    fn try_next_id_async<S>(&self) -> impl Future<Output = Result<ID, Self::Err>>
    where
        S: SleepProvider;
}

impl<G, ID, T> SnowflakeGeneratorAsyncExt<ID, T> for G
where
    G: SnowflakeGenerator<ID, T> + Sync,
    ID: SnowflakeId + Send,
    T: TimeSource<ID::Ty> + Send,
{
    type Err = G::Err;

    async fn next_id_async<S>(&self) -> ID
    where
        S: SleepProvider,
        Self::Err: Into<Infallible>,
    {
        match self.try_next_id_async::<S>().await {
            Ok(id) => id,
            Err(e) => {
                #[allow(unreachable_code)]
                // `into()` satisfies the trait bound at compile time.
                match e.into() {}
            }
        }
    }

    async fn try_next_id_async<S>(&self) -> Result<ID, Self::Err>
    where
        S: SleepProvider,
    {
        loop {
            let dur = match self.try_poll_id()? {
                Poll::Ready { id } => break Ok(id),
                Poll::Pending { yield_for } => {
                    Duration::from_millis(yield_for.to_u64().saturating_mul(T::GRANULARITY_MILLIS))
                }
            };
            S::sleep_for(dur).await;
        }
    }
}

#[cfg(all(test, feature = "lock"))]
mod tests {
    use core::{
        pin::pin,
        task::{Context, Poll as TaskPoll, RawWaker, RawWakerVTable, Waker},
    };
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    use super::*;
    use crate::{generator::LockSnowflakeGenerator, id::SnowflakeTwitterId};

    static LAST_SLEEP: Mutex<Option<Duration>> = Mutex::new(None);

    struct RecordingSleep;

    impl SleepProvider for RecordingSleep {
        fn sleep_for(dur: Duration) -> impl Future<Output = ()> + Send {
            async move {
                *LAST_SLEEP.lock().unwrap() = Some(dur);
            }
        }
    }

    #[derive(Default)]
    struct CoarseStepTime {
        reads: AtomicUsize,
    }

    impl TimeSource<u64> for CoarseStepTime {
        const GRANULARITY_MILLIS: u64 = 8;

        fn current_millis(&self) -> u64 {
            self.reads.fetch_add(1, Ordering::Relaxed) as u64
        }
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        fn raw_waker() -> RawWaker {
            fn clone(_: *const ()) -> RawWaker {
                raw_waker()
            }
            fn wake(_: *const ()) {}
            fn wake_by_ref(_: *const ()) {}
            fn drop(_: *const ()) {}

            RawWaker::new(
                core::ptr::null(),
                &RawWakerVTable::new(clone, wake, wake_by_ref, drop),
            )
        }

        let waker = unsafe { Waker::from_raw(raw_waker()) };
        let mut context = Context::from_waker(&waker);
        let mut future = pin!(future);

        loop {
            match future.as_mut().poll(&mut context) {
                TaskPoll::Ready(value) => break value,
                TaskPoll::Pending => core::hint::spin_loop(),
            }
        }
    }

    #[test]
    fn async_backoff_uses_time_source_granularity() {
        *LAST_SLEEP.lock().unwrap() = None;

        let generator: LockSnowflakeGenerator<SnowflakeTwitterId, _> =
            LockSnowflakeGenerator::from_components(
                0,
                0,
                SnowflakeTwitterId::max_sequence(),
                CoarseStepTime::default(),
            );

        let id = block_on(
            <LockSnowflakeGenerator<SnowflakeTwitterId, _> as SnowflakeGeneratorAsyncExt<
                SnowflakeTwitterId,
                _,
            >>::try_next_id_async::<RecordingSleep>(&generator),
        )
        .unwrap();

        assert_eq!(*LAST_SLEEP.lock().unwrap(), Some(Duration::from_millis(8)));
        assert_eq!(id.timestamp(), 1);
    }
}
