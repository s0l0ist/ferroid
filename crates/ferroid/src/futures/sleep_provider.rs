use core::time::Duration;

/// A trait that abstracts over how to sleep for a given [`Duration`] in async
/// contexts.
///
/// This allows the generator to be generic over runtimes like `Tokio` or
/// `Smol`.
pub trait SleepProvider {
    /// We require `Send` so that the future can be safely moved across threads
    type Sleep: Future<Output = ()> + Send;

    fn sleep_for(dur: Duration) -> Self::Sleep;
}
