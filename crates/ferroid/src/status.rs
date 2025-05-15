use crate::Snowflake;

/// Represents the result of attempting to generate a new Snowflake ID.
///
/// This type models the outcome of `SnowflakeGenerator::try_next_id()`:
///
/// - [`IdGenStatus::Ready`] indicates a new ID was successfully generated.
/// - [`IdGenStatus::Pending`] means the generator is throttled and cannot
///   produce a new ID until the clock advances past `yield_until`.
///
/// This allows non-blocking generation loops and clean backoff strategies.
///
/// # Example
///
/// ```
/// use ferroid::{Snowflake, BasicSnowflakeGenerator, SnowflakeTwitterId, IdGenStatus};
///
/// struct FixedTime;
/// impl ferroid::TimeSource<u64> for FixedTime {
///     fn current_millis(&self) -> u64 {
///         1
///     }
/// }
///
/// let mut generator = BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::from_components(0, 1, SnowflakeTwitterId::max_sequence(), FixedTime);
/// match generator.next_id() {
///     IdGenStatus::Ready { id } => println!("ID: {}", id.timestamp()),
///     IdGenStatus::Pending { yield_until } => println!("Back off until: {yield_until}"),
/// }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdGenStatus<T: Snowflake> {
    /// A unique ID was generated and is ready to use.
    Ready {
        /// The generated Snowflake ID.
        id: T,
    },
    /// No ID could be generated because the sequence has been exhausted for the
    /// current tick.
    ///
    /// You should wait until the clock reaches or exceeds `yield_until` before
    /// attempting to generate a new ID again.
    Pending {
        /// The next timestamp (inclusive) at which you may resume generating
        /// IDs.
        yield_until: T::Ty,
    },
}
