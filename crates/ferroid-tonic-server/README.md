# A gRPC Server for Streaming Snowflake ID Generation

`ferroid-tonic-server` is a high-performance, streaming gRPC server for batch
Snowflake-style ID generation, built with
[`tonic`](https://github.com/hyperium/tonic) and powered by
[`ferroid`](https://github.com/s0l0ist/ferroid).

This server is optimized for latency-sensitive and high-throughput workloads
such as distributed queues, event ingestion pipelines, or scalable database key
generation-where time-ordered, collision-resistant IDs are critical.

## Features

- **Streaming gRPC Interface**: Clients request batches of IDs via the
  `StreamIds` endpoint; IDs are streamed back in chunks (compression optional).
- **Zstd, Gzip, Deflate Compression**: Negotiated via gRPC per stream.
- **Per-Worker ID Generators**: Each async task owns its own generator and shard
  (Snowflake `worker_id`), ensuring scale-out safety and eliminating contention.
- **Backpressure-aware**: Bounded queues prevent unbounded memory growth.
- **Graceful Shutdown**: Ensures all in-flight work completes.
- **Client Cancellation**: Stream requests are interruptible mid-flight.

## Running the Server

Install:

```bash

# Install with the `tracing` feature to see traces/logs adjustable with RUST_LOG
cargo install ferroid-tonic-server --features tracing

# If you want full telemetry, it supports exporting to Honeycomb
cargo install ferroid-tonic-server --features tracing,metrics,honeycomb
```

Run with a specific number of workers. Each worker corresponds to the
`machine_id` of the Snowflake ID:

```bash
ferroid-tonic-server --num-workers 16
```

The server listens on `0.0.0.0:50051` by default. You can override the address
via CLI or environment variables (see `--help`).

## Example: List Services via Reflection

```bash
grpcurl -plaintext localhost:50051 list

> ferroid.IdGenerator grpc.reflection.v1.ServerReflection
```

You can run an example query, but the results are in base64 binary packed form
from grpcurl. To deserialize properly, checkout the benchmarks:

```bash
grpcurl -plaintext \
-d '{"count": 1}' \
localhost:50051 \
ferroid.IdGenerator/StreamIds

> { "packedIds": "AADANUc+1AA=" }
```

## Healthcheck

```bash
./grpc-health-probe -addr=localhost:50051 -service=ferroid.IdGenerator

> status: SERVING
```

## Integration

- Import the `.proto` file from `ferroid-tonic`
- Use `IdGenerator.StreamIds` for streaming ID allocation
- Each response chunk (`IdChunk`) contains a packed byte buffer of IDs

### ⚠️ Note

- ID size (e.g., `u64`, `u128`) must be inferred by the client
- IDs are packed in little-endian binary format (see `IdChunk.packed_ids`)
