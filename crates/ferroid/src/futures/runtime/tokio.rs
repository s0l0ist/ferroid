use crate::futures::SleepProvider;
use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;

/// An implementation of [`SleepProvider`] using Tokio's timer.
///
/// This is the default provider for use in async applications built on Tokio.
pub struct TokioSleep;
impl SleepProvider for TokioSleep {
    type Sleep = tokio::time::Sleep;

    fn sleep_for(dur: core::time::Duration) -> Self::Sleep {
        tokio::time::sleep(dur)
    }
}

/// An implementation of [`SleepProvider`] using Tokio's yield.
///
/// This strategy avoids timer-based delays by yielding to the scheduler
/// immediately, which can improve responsiveness in low-concurrency scenarios.
///
/// However, it comes at the cost of more frequent rescheduling, which can
/// result in tighter polling loops and increased CPU usage under load. In
/// highly concurrent cases, a timer-based sleep (e.g., [`TokioSleep`]) is often
/// more efficient due to reduced scheduler churn.
pub struct TokioYield;
impl SleepProvider for TokioYield {
    /// Tokio's `yield_now()` returns a private future type, so we must use a
    /// boxed `dyn Future` to abstract over it.
    type Sleep = Pin<Box<dyn Future<Output = ()> + Send>>;

    fn sleep_for(_dur: core::time::Duration) -> Self::Sleep {
        Box::pin(tokio::task::yield_now())
    }
}
