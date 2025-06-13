use anyhow::bail;
use clap::Parser;
use ferroid_tonic::common::types::{SNOWFLAKE_ID_SIZE, SnowflakeId};

/// Runtime configuration for the `id-server` binary.
///
/// These settings control the concurrency, buffering, and chunking behavior of
/// the Snowflake ID generation service. All values are parsed from CLI
/// arguments or environment variables, with reasonable defaults suitable for
/// production.
///
/// Each field is independently tunable at runtime, allowing for flexible
/// deployment in clusters of varying sizes, memory constraints, or throughput
/// needs.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "tonic-server",
    version,
    about = "A gRPC service for streaming Snowflake-like IDs"
)]
pub struct CliArgs {
    /// Maximum number of Snowflake IDs allowed per client request.
    ///
    /// This value is enforced server-side to prevent abuse or memory exhaustion
    /// from overly large requests. Clients may request fewer IDs, and large
    /// requests are automatically chunked into smaller units.
    ///
    /// Environment variable: `MAX_ALLOWED_IDS`
    #[arg(long, env = "MAX_ALLOWED_IDS", default_value_t = 1_000_000_000)]
    pub max_allowed_ids: usize,

    /// Offset used to shard machine ID space across multiple deployments or
    /// tenants.
    ///
    /// This value is added to the worker index to compute each generator's
    /// unique machine ID. Use this to avoid ID collisions in multi-region or
    /// multi-tenant environments sharing a global ID namespace.
    ///
    /// Environment variable: `SHARD_OFFSET`
    #[arg(long, env = "SHARD_OFFSET", default_value_t = 0)]
    pub shard_offset: usize,

    /// Number of worker tasks responsible for generating IDs concurrently.
    ///
    /// Each worker is assigned a unique machine ID derived from its index and
    /// the `shard_offset`. The value must not exceed the bit width of the
    /// machine ID field in the Snowflake format.
    ///
    /// Environment variable: `NUM_WORKERS`
    #[arg(long, env = "NUM_WORKERS", default_value_t = 1)]
    pub num_workers: usize,

    /// Number of Snowflake IDs included in each response chunk.
    ///
    /// Defines the size of each `IdChunk`. Ideally, this aligns
    /// with the maximum sequence value of the Snowflake ID type. The default
    /// assumes a 12-bit sequence. While the gRPC protocol does impose message
    /// size limits, these are only a concern when using IDs with high sequence
    /// bit allocations.
    ///
    /// Environment variable: `IDS_PER_CHUNK`
    #[arg(long, env = "IDS_PER_CHUNK", default_value_t = 4096)]
    pub ids_per_chunk: usize,

    /// Capacity of the response buffer between worker and gRPC stream.
    ///
    /// This affects how many response chunks can be buffered before the worker
    /// must wait for the client to consume more data. Lower values increase
    /// backpressure responsiveness; higher values enable deeper pipelining.
    ///
    /// Environment variable: `STREAM_BUFFER_SIZE`
    #[arg(long, env = "STREAM_BUFFER_SIZE", default_value_t = 8)]
    pub stream_buffer_size: usize,

    /// Address to listen on (TCP or Unix socket path; use --uds for Unix socket).
    ///
    /// Example: "0.0.0.0:50051" or "/tmp/tonic-uds.sock"
    ///
    /// Environment variable: `SERVER_ADDR`
    #[arg(long, env = "SERVER_ADDR", default_value_t = String::from("0.0.0.0:50051"))]
    pub server_addr: String,

    /// Listen on a Unix socket instead of TCP. If set, `SERVER_ADDR` must be a file path.
    #[arg(short, long, default_value_t = false)]
    pub uds: bool,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub max_allowed_ids: usize,
    pub shard_offset: usize,
    pub num_workers: usize,
    pub ids_per_chunk: usize,
    pub stream_buffer_size: usize,
    pub chunk_bytes: usize,
    pub server_addr: String,
    pub uds: bool,
}

impl TryFrom<CliArgs> for ServerConfig {
    type Error = anyhow::Error;

    fn try_from(args: CliArgs) -> Result<Self, Self::Error> {
        let max_machine_id = SnowflakeId::max_machine_id() as usize + 1;

        if args.num_workers == 0 {
            bail!("NUM_WORKERS must be greater than 0");
        }

        if args.num_workers > max_machine_id {
            bail!(
                "NUM_WORKERS ({}) exceeds available Snowflake machine ID space (max = {})",
                args.num_workers,
                max_machine_id
            );
        }

        let chunk_bytes = args
            .ids_per_chunk
            .checked_mul(SNOWFLAKE_ID_SIZE)
            .ok_or_else(|| anyhow::anyhow!("Overflow in chunk_bytes computation"))?;

        Ok(Self {
            max_allowed_ids: args.max_allowed_ids,
            shard_offset: args.shard_offset,
            num_workers: args.num_workers,
            ids_per_chunk: args.ids_per_chunk,
            stream_buffer_size: args.stream_buffer_size,
            server_addr: args.server_addr,
            chunk_bytes,
            uds: args.uds,
        })
    }
}
