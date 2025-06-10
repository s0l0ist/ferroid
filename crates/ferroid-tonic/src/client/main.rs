use anyhow::Result;
use ferroid::Snowflake;
use ferroid_tonic::common::{
    idgen::{IdStreamRequest, id_gen_client::IdGenClient},
    types::{SNOWFLAKE_ID_SIZE, SnowflakeIdTy, SnowflakeIdType},
};
use futures::stream::{FuturesUnordered, StreamExt as FutureStreamExt};
use std::time::{Duration, Instant};
use tokio_stream::StreamExt as TokioStreamExt;
use tonic::{codec::CompressionEncoding, transport::Channel};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

const PARALLEL: usize = 50;

#[tokio::main]
async fn main() -> Result<()> {
    let sequential_cases = [
        1,
        10,
        100,
        1_000,
        10_000,
        100_000,
        1_000_000,
        10_000_000,
        100_000_000,
    ];
    let parallel_cases = sequential_cases;

    let mut results = Vec::new();

    println!("\n=== Running Sequential ===");
    for &count in &sequential_cases {
        results.push(run_stream_bench(count, 1).await?);
    }

    println!("\n=== Running Parallel ({PARALLEL} tasks) ===");
    for &count in &parallel_cases {
        results.push(run_stream_bench(count, PARALLEL).await?);
    }

    // === Final Summary Table ===
    println!("\n=== Benchmark Summary ===");
    println!(
        "{:<22} | {:>10} | {:>10} | {:>10} | {:>12}",
        "Method", "Count", "Total", "Time (ms)", "Throughput/s"
    );
    println!("{}", "-".repeat(72));
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
            "{:<22} | {:>10} | {:>10} | {:>10.2} | {:>12.2}",
            self.label,
            self.target_count,
            self.total_received,
            self.duration.as_secs_f64() * 1000.0,
            self.throughput()
        );
    }
}

async fn run_stream_bench(target_count: u64, concurrency: usize) -> Result<BenchmarkResult> {
    let start = Instant::now();
    let mut tasks = FuturesUnordered::new();

    for _ in 0..concurrency {
        tasks.push(tokio::spawn(async move {
            let channel = Channel::from_static("http://localhost:50051")
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

                if bytes.len() % SNOWFLAKE_ID_SIZE != 0 {
                    anyhow::bail!(
                        "Corrupt chunk: not a multiple of {SNOWFLAKE_ID_SIZE}, got {} bytes",
                        bytes.len()
                    );
                }

                for chunk in bytes.chunks_exact(SNOWFLAKE_ID_SIZE) {
                    let raw_id = SnowflakeIdTy::from_le_bytes(chunk.try_into().unwrap());
                    let _id = SnowflakeIdType::from_raw(raw_id);
                    received += 1;
                }
            }
            Ok::<usize, anyhow::Error>(received)
        }));
    }

    let mut total_received = 0;
    while let Some(res) = FutureStreamExt::next(&mut tasks).await {
        total_received += res??;
    }

    let duration = start.elapsed();

    Ok(BenchmarkResult {
        label: format!(
            "{} stream x{}",
            if concurrency == 1 {
                "Sequential"
            } else {
                "Parallel"
            },
            concurrency
        ),
        target_count: target_count as usize,
        total_received,
        duration,
    })
}
