use smol::Timer;

use crate::futures::SleepProvider;

/// An implementation of [`SleepProvider`] using Smol's timer.
///
/// This is the default provider for use in async applications built on Smol.
pub struct SmolSleep;
impl SleepProvider for SmolSleep {
    async fn sleep_for(dur: core::time::Duration) {
        Timer::after(dur).await;
    }
}

/// An implementation of [`SleepProvider`] using Smol's yield.
///
/// This strategy avoids timer-based delays by yielding to the scheduler
/// immediately, which can improve responsiveness in low-concurrency scenarios.
///
/// However, it comes at the cost of more frequent rescheduling, which can
/// result in tighter polling loops and increased CPU usage under load. In
/// highly concurrent cases, a timer-based sleep (e.g., [`SmolSleep`]) is often
/// more efficient due to reduced scheduler churn.
pub struct SmolYield;
impl SleepProvider for SmolYield {
    async fn sleep_for(_dur: core::time::Duration) {
        smol::future::yield_now().await;
    }
}
