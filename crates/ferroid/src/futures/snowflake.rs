use super::SleepProvider;
use crate::{IdGenStatus, Result, SnowflakeGenerator, SnowflakeId, TimeSource, ToU64};
use core::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use pin_project_lite::pin_project;

/// Extension trait for asynchronously generating Snowflake IDs.
///
/// This trait enables `SnowflakeGenerator` types to yield IDs in a
/// `Future`-based context by awaiting until the generator is ready to produce a
/// new ID.
///
/// The default implementation uses [`SnowflakeGeneratorFuture`] and a specified
/// [`SleepProvider`] to yield when the generator is not yet ready.
pub trait SnowflakeGeneratorAsyncExt<ID, T>
where
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err;

    /// Returns a future that resolves to the next available Snowflake ID.
    ///
    /// If the generator is not ready to issue a new ID immediately, the future
    /// will sleep for the amount of time indicated by the generator and retry.
    ///
    /// # Errors
    ///
    /// This future may return an error if the generator encounters one.
    fn try_next_id_async<S>(&self) -> impl Future<Output = Result<ID, Self::Err>>
    where
        S: SleepProvider;
}

impl<G, ID, T> SnowflakeGeneratorAsyncExt<ID, T> for G
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
{
    type Err = G::Err;

    #[allow(clippy::future_not_send)]
    fn try_next_id_async<'a, S>(&'a self) -> impl Future<Output = Result<ID, Self::Err>>
    where
        S: SleepProvider,
    {
        SnowflakeGeneratorFuture::<'a, G, ID, T, S>::new(self)
    }
}

pin_project! {
    /// A future that polls a [`SnowflakeGenerator`] until it is ready to
    /// produce an ID.
    ///
    /// This future handles `Pending` responses by sleeping for a recommended
    /// amount of time before polling the generator again.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct SnowflakeGeneratorFuture<'a, G, ID, T, S>
    where
        G: SnowflakeGenerator<ID, T>,
        ID: SnowflakeId,
        T: TimeSource<ID::Ty>,
        S: SleepProvider,
    {
        generator: &'a G,
        #[pin]
        sleep: Option<S::Sleep>,
        _idt: PhantomData<(ID, T)>
    }
}

impl<'a, G, ID, T, S> SnowflakeGeneratorFuture<'a, G, ID, T, S>
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
    S: SleepProvider,
{
    /// Constructs a new [`SnowflakeGeneratorFuture`] from a given generator.
    ///
    /// This does not immediately begin polling the generator; instead, it will
    /// attempt to produce an ID when `.poll()` is called.
    pub const fn new(generator: &'a G) -> Self {
        Self {
            generator,
            sleep: None,
            _idt: PhantomData,
        }
    }
}
impl<G, ID, T, S> Future for SnowflakeGeneratorFuture<'_, G, ID, T, S>
where
    G: SnowflakeGenerator<ID, T>,
    ID: SnowflakeId,
    T: TimeSource<ID::Ty>,
    S: SleepProvider,
{
    type Output = Result<ID, G::Err>;

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
        }
        match this.generator.try_next_id()? {
            IdGenStatus::Ready { id } => Poll::Ready(Ok(id)),
            IdGenStatus::Pending { yield_for } => {
                let sleep_fut = S::sleep_for(Duration::from_millis(yield_for.to_u64()));
                this.sleep.as_mut().set(Some(sleep_fut));
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }
}
