use core::{convert::Infallible, future::Future, time::Duration};

use crate::{
    generator::{Poll, Result, SnowflakeGenerator},
    id::SnowflakeId,
    time::TimeSource,
};

/// Extension trait for asynchronous Snowflake generation.
///
/// Uses a caller-supplied async backoff strategy when generation is pending.
///
/// This interface needs neither `std` nor allocation. Local generators and
/// callbacks are supported; the returned future is `Send` when its captures
/// and the callback's future are `Send`.
///
/// Generic callers can require a `Send` future when the generator is shared
/// and the callback and its future are `Send`:
///
/// # Example
/// ```
/// use core::{future::Future, time::Duration};
/// use ferroid::{
///     futures::SnowflakeGeneratorAsyncExt,
///     id::SnowflakeTwitterId,
///     time::TimeSource,
/// };
///
/// fn send_future<G, T, B, F>(
///     generator: &G,
///     f: B,
/// ) -> impl Future<Output = Result<SnowflakeTwitterId, G::Err>> + Send
/// where
///     G: ferroid::generator::SnowflakeGenerator<SnowflakeTwitterId, T> + Sync,
///     T: TimeSource<u64>,
///     B: FnMut(Duration) -> F + Send,
///     F: Future<Output = ()> + Send,
/// {
///     generator.try_next_id_async(f)
/// }
/// ```
pub trait SnowflakeGeneratorAsyncExt<ID, T>: SnowflakeGenerator<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    /// Waits for the next ID from an infallible generator.
    ///
    /// `f` receives a suggested [`Duration`], including [`Duration::ZERO`],
    /// with clock granularity already applied. It may sleep, yield, or perform
    /// any other async work. The generator awaits it before retrying. The
    /// delay is advisory; waits saturate at `u64::MAX`
    /// milliseconds.
    fn next_id_async<B, F>(&self, f: B) -> impl Future<Output = ID>
    where
        B: FnMut(Duration) -> F,
        F: Future<Output = ()>,
        Self::Err: Into<Infallible>;

    /// Waits for the next ID, propagating generator errors immediately.
    ///
    /// On [`Poll::Pending`], awaits `f` with the suggested [`Duration`]
    /// before polling again. The callback may perform any async work and may
    /// ignore the delay.
    /// Cancelling this future stops retrying; it does not reserve an ID.
    ///
    /// # Errors
    ///
    /// Returns the underlying generator error, such as mutex poisoning.
    fn try_next_id_async<B, F>(&self, f: B) -> impl Future<Output = Result<ID, Self::Err>>
    where
        B: FnMut(Duration) -> F,
        F: Future<Output = ()>;
}

impl<G, ID, T> SnowflakeGeneratorAsyncExt<ID, T> for G
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    async fn next_id_async<B, F>(&self, f: B) -> ID
    where
        B: FnMut(Duration) -> F,
        F: Future<Output = ()>,
        Self::Err: Into<Infallible>,
    {
        let Ok(id) = self
            .try_next_id_async(f)
            .await
            .map_err(Into::<Infallible>::into);
        id
    }

    async fn try_next_id_async<B, F>(&self, mut f: B) -> Result<ID, Self::Err>
    where
        B: FnMut(Duration) -> F,
        F: Future<Output = ()>,
    {
        loop {
            let delay = match self.try_poll_id()? {
                Poll::Ready { id } => return Ok(id),
                Poll::Pending { yield_for } => yield_for,
            };
            f(delay).await;
        }
    }
}
