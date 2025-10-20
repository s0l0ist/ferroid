use crate::Id;

/// Represents the result of attempting to generate a new Snowflake ID.
///
/// This type models the outcome of `SnowflakeGenerator::try_next_id()`:
///
/// - [`IdGenStatus::Ready`] indicates a new ID was successfully generated.
/// - [`IdGenStatus::Pending`] means the generator is throttled and cannot
///   produce a new ID until the clock advances past `yield_for`.
///
/// This allows non-blocking generation loops and clean backoff strategies.
///
/// # Example
/// ```
/// use ferroid::{SnowflakeId, BasicSnowflakeGenerator, SnowflakeTwitterId, IdGenStatus};
///
/// struct FixedTime;
/// impl ferroid::TimeSource<u64> for FixedTime {
///     fn current_millis(&self) -> u64 {
///         1
///     }
/// }
///
/// let generator = BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(0, 1, SnowflakeTwitterId::max_sequence(), FixedTime);
/// match generator.next_id() {
///     IdGenStatus::Ready { id } => println!("ID: {}", id.timestamp()),
///     IdGenStatus::Pending { yield_for } => println!("Back off for: {yield_for}"),
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdGenStatus<T: Id> {
    /// A new Snowflake ID was successfully generated.
    Ready {
        /// The generated Snowflake ID.
        id: T,
    },
    /// The generator is not ready to produce a new ID yet.
    ///
    /// Wait for the specified number of milliseconds (`yield_for`) before
    /// trying again.
    Pending {
        /// Milliseconds to wait before the next attempt.
        yield_for: T::Ty,
    },
}
