use core::{future::Future, time::Duration};

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

impl<G, ID, T, R> UlidGeneratorAsyncExt<ID, T, R> for G
where
    G: UlidGenerator<ID, T, R> + Sync,
    ID: UlidId + Send,
    T: TimeSource<ID::Ty> + Send,
    R: RandSource<ID::Ty> + Send,
{
    type Err = G::Err;

    fn try_next_id_async<S>(&self) -> impl Future<Output = Result<ID, Self::Err>>
    where
        S: SleepProvider,
    {
        async {
            loop {
                let dur = match self.try_next_id()? {
                    IdGenStatus::Ready { id } => return Ok(id),
                    IdGenStatus::Pending { yield_for } => Duration::from_millis(yield_for.to_u64()),
                };
                S::sleep_for(dur).await;
            }
        }
    }
}
