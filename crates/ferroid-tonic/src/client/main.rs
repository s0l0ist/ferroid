use ferroid::Snowflake;
use ferroid_tonic::common::types::{SNOWFLAKE_ID_SIZE, SnowflakeIdTy, SnowflakeIdType};
use futures::stream::{FuturesUnordered, StreamExt as FuturesStreamExt};
use idgen::{IdStreamRequest, id_gen_client::IdGenClient};
use std::time::{Duration, Instant};
use tokio_stream::StreamExt as TokioStreamExt;
use tonic::{codec::CompressionEncoding, transport::Channel};

pub mod idgen {
    tonic::include_proto!("idgen");
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut results = Vec::new();

    println!("\n=== Running Sequential ===");
    results.push(run_parallel_stream(1, 1).await?);
    results.push(run_parallel_stream(10, 1).await?);
    results.push(run_parallel_stream(100, 1).await?);
    results.push(run_parallel_stream(1000, 1).await?);
    results.push(run_parallel_stream(10000, 1).await?);
    results.push(run_parallel_stream(100000, 1).await?);
    results.push(run_parallel_stream(1000000, 1).await?);
    results.push(run_parallel_stream(10000000, 1).await?);
    results.push(run_parallel_stream(100000000, 1).await?);
    // results.push(run_parallel_stream(1000000000, 1).await?);
    println!("\n=== Running Parallel ===");

    results.push(run_parallel_stream(1, 50).await?);
    results.push(run_parallel_stream(10, 50).await?);
    results.push(run_parallel_stream(100, 50).await?);
    results.push(run_parallel_stream(1000, 50).await?);
    results.push(run_parallel_stream(10000, 50).await?);
    results.push(run_parallel_stream(100000, 50).await?);
    results.push(run_parallel_stream(1000000, 50).await?);
    results.push(run_parallel_stream(10000000, 50).await?);
    results.push(run_parallel_stream(100000000, 50).await?);
    // results.push(run_parallel_stream(1000000000, 50).await?);

    // === Final Summary Table ===
    println!("\n=== Benchmark Summary ===");
    println!(
        "{:<25} | {:>10} | {:>10} | {:>15}",
        "Method", "Count", "Time (ms)", "Throughput (/s)"
    );
    println!("{}", "-".repeat(65));
    for r in &results {
        r.report();
    }

    Ok(())
}

#[derive(Debug)]
struct BenchmarkResult {
    label: String,
    target_count: usize,
    total_received: usize,
    duration: Duration,
}

impl BenchmarkResult {
    fn throughput(&self) -> f64 {
        self.total_received as f64 / self.duration.as_secs_f64()
    }

    fn report(&self) {
        println!(
            "{:<25} | {:>10} target | {:>10} total | {:>8.2} ms | {:>10.2} ID/sec",
            self.label,
            self.target_count,
            self.total_received,
            self.duration.as_secs_f64() * 1000.0,
            self.throughput()
        );
    }
}

async fn run_parallel_stream(
    target_count: u64,
    concurrency: usize,
) -> Result<BenchmarkResult, Box<dyn std::error::Error + Send + Sync>> {
    // let per_stream = target_count / concurrency as u64;
    let start = Instant::now();

    let mut tasks = FuturesUnordered::new();

    for _ in 0..concurrency {
        tasks.push(tokio::spawn(async move {
            let channel = Channel::from_static("http://127.0.0.1:50051")
                .connect()
                .await?;
            let mut client = IdGenClient::new(channel)
                // .accept_compressed(CompressionEncoding::Zstd)
                .send_compressed(CompressionEncoding::Zstd);

            let mut stream = client
                .get_stream_ids(IdStreamRequest {
                    count: target_count,
                })
                .await?
                .into_inner();

            let mut received = 0;
            while let Some(resp) = TokioStreamExt::next(&mut stream).await {
                let raw = resp?.packed_ids;
                let bytes = raw.as_ref();

                assert_eq!(
                    bytes.len() % SNOWFLAKE_ID_SIZE,
                    0,
                    "Corrupt chunk: not a multiple of {SNOWFLAKE_ID_SIZE}"
                );

                for chunk in bytes.chunks_exact(SNOWFLAKE_ID_SIZE) {
                    let raw_id = SnowflakeIdTy::from_le_bytes(chunk.try_into().unwrap());
                    let _id = SnowflakeIdType::from_raw(raw_id);
                    received += 1;
                }
            }

            Ok::<usize, Box<dyn std::error::Error + Send + Sync>>(received)
        }));
    }

    // Collect all results
    let mut total_received = 0;
    while let Some(res) = FuturesStreamExt::next(&mut tasks).await {
        let i = res??;
        total_received += i;
    }

    let duration = start.elapsed();

    Ok(BenchmarkResult {
        label: format!("Parallel stream x{}", concurrency),
        target_count: target_count as usize,
        total_received,
        duration,
    })
}
