use core::{convert::Infallible, future::Future, time::Duration};

use super::SleepProvider;
use crate::{
    generator::{IdGenStatus, Result, UlidGenerator},
    id::{ToU64, UlidId},
    rand::RandSource,
    time::TimeSource,
};

/// Extension trait for asynchronously generating ULIDs.
///
/// This trait enables `UlidGenerator` types to yield IDs in a `Future`-based
/// context by awaiting until the generator is ready to produce a new ID.
///
/// The default implementation uses [`UlidGeneratorFuture`] and a specified
/// [`SleepProvider`] to yield when the generator is not yet ready.
pub trait UlidGeneratorAsyncExt<ID, T, R>
where
    ID: UlidId,
    T: TimeSource<ID::Ty>,
    R: RandSource<ID::Ty>,
{
    type Err;

    /// Returns a future that resolves to the next available ULID.
    ///
    /// This infallible method automatically retries when the generator is
    /// temporarily unable to produce an ID. Only available for generators with
    /// infallible error types.
    ///
    /// For fallible generators, use [`Self::try_next_id_async`]
    fn next_id_async<S>(&self) -> impl Future<Output = ID>
    where
        S: SleepProvider,
        Self::Err: Into<Infallible>;

    /// Returns a future that resolves to the next available ULID.
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
            Err(e) =>
            {
                #[allow(unreachable_code)]
                match e.into() {}
            }
        }
    }

    async fn try_next_id_async<S>(&self) -> Result<ID, Self::Err>
    where
        S: SleepProvider,
    {
        loop {
            let dur = match self.try_next_id()? {
                IdGenStatus::Ready { id } => return Ok(id),
                IdGenStatus::Pending { yield_for } => Duration::from_millis(yield_for.to_u64()),
            };
            S::sleep_for(dur).await;
        }
    }
}
