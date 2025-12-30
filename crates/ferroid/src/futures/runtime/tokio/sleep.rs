use crate::futures::SleepProvider;

/// An implementation of [`SleepProvider`] using Tokio's timer.
///
/// This is the default provider for use in async applications built on Tokio.
pub struct TokioSleep;
impl SleepProvider for TokioSleep {
    async fn sleep_for(dur: core::time::Duration) {
        tokio::time::sleep(dur).await
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
    async fn sleep_for(_dur: core::time::Duration) {
        tokio::task::yield_now().await
    }
}
