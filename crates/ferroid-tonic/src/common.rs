use ferroid::{Snowflake, SnowflakeTwitterId};

/// ID type used by both cliend and server instances
pub type SnowflakeIdType = SnowflakeTwitterId;

/// Size in bytes of each Snowflake ID when serialized.
pub type SnowflakeIdTy = <SnowflakeIdType as Snowflake>::Ty;

/// Size in bytes of each Snowflake ID when serialized.
pub const SNOWFLAKE_ID_SIZE: usize = size_of::<SnowflakeIdTy>();
