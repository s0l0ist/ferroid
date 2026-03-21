use core::{convert::Infallible, future::Future, time::Duration};

use super::SleepProvider;
use crate::{
    generator::{Poll, Result, UlidGenerator},
    id::{ToU64, UlidId},
    rand::RandSource,
    time::TimeSource,
};

/// Extension trait for asynchronously generating ULIDs.
///
/// This trait enables `UlidGenerator` types to yield IDs in a `Future`-based
/// context by awaiting until the generator is ready to produce a new ID.
pub trait UlidGeneratorAsyncExt<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
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

impl<G, ID, T, R> UlidGeneratorAsyncExt<ID, T, R> for G
where
    G: UlidGenerator<ID, T, R> + Sync,
    ID: UlidId + Send,
    T: TimeSource<ID::Ty> + Send,
    R: RandSource<ID::Ty> + Send,
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
    use crate::{generator::LockMonoUlidGenerator, id::ULID};

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

    impl TimeSource<u128> for CoarseStepTime {
        const GRANULARITY_MILLIS: u64 = 8;

        fn current_millis(&self) -> u128 {
            self.reads.fetch_add(1, Ordering::Relaxed) as u128
        }
    }

    #[derive(Clone, Copy)]
    struct ZeroRand;

    impl RandSource<u128> for ZeroRand {
        fn rand(&self) -> u128 {
            0
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

        let generator: LockMonoUlidGenerator<ULID, _, _> = LockMonoUlidGenerator::from_components(
            0,
            ULID::max_random(),
            CoarseStepTime::default(),
            ZeroRand,
        );

        let id = block_on(
            <LockMonoUlidGenerator<ULID, _, _> as UlidGeneratorAsyncExt<
                ULID,
                _,
                _,
            >>::try_next_id_async::<RecordingSleep>(&generator),
        )
        .unwrap();

        assert_eq!(*LAST_SLEEP.lock().unwrap(), Some(Duration::from_millis(8)));
        assert_eq!(id.timestamp(), 1);
    }
}
