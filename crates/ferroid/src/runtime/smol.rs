use crate::SleepProvider;
use core::{
    pin::Pin,
    task::{Context, Poll},
};
use pin_project_lite::pin_project;
use smol::Timer;

/// An implementation of [`SleepProvider`] using Smol's timer.
///
/// This is the default provider for use in async applications built on Smol.
pub struct SmolSleep;
impl SleepProvider for SmolSleep {
    type Sleep = SmolSleepFuture;

    fn sleep_for(dur: core::time::Duration) -> Self::Sleep {
        SmolSleepFuture {
            timer: Timer::after(dur),
        }
    }
}

pin_project! {
    /// Internal future returned by [`SmolSleep::sleep_for`].
    ///
    /// This type wraps a [`smol::Timer`] and implements [`Future`] with `Output
    /// = ()`, discarding the timer's `Instant` result.
    ///
    /// You should not construct or use this type directly. It is only used
    /// internally by the [`SleepProvider`] implementation for the Smol runtime.
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
    type Sleep = smol::future::YieldNow;

    fn sleep_for(_dur: core::time::Duration) -> Self::Sleep {
        smol::future::yield_now()
    }
}
