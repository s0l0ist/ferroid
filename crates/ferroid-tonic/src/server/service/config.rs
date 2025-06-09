use ferroid::{BasicSnowflakeGenerator, MonotonicClock};
use ferroid_tonic::common::types::SnowflakeIdType;

/// Clock implementation used by all Snowflake generators.
///
/// This controls how timestamps are embedded into generated IDs.
pub type ClockType = MonotonicClock;

/// Default Snowflake generator used per worker task.
///
/// Each instance is parameterized with a unique machine ID and shared clock.
pub type SnowflakeGeneratorType = BasicSnowflakeGenerator<SnowflakeIdType, ClockType>;
