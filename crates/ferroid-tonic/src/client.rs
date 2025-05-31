use ferroid::{Snowflake, SnowflakeTwitterId};
use futures::stream::{FuturesUnordered, StreamExt as FuturesStreamExt};
use idgen::{IdStreamRequest, id_gen_client::IdGenClient};
use std::time::{Duration, Instant};
use tokio_stream::StreamExt as TokioStreamExt;
use tonic::{codec::CompressionEncoding, transport::Channel};

pub mod idgen {
    tonic::include_proto!("idgen");
}

#[derive(Debug)]
struct BenchmarkResult {
    label: String,
    count: usize,
    duration: Duration,
}

impl BenchmarkResult {
    fn throughput(&self) -> f64 {
        self.count as f64 / self.duration.as_secs_f64()
    }

    fn report(&self) {
        println!(
            "{:<25} | {:>10} items | {:>8.2} ms | {:>10.2} items/sec",
            self.label,
            self.count,
            self.duration.as_secs_f64() * 1000.0,
            self.throughput()
        );
    }
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    results.push(run_parallel_stream(1000000000, 50).await?);

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

async fn run_parallel_stream(
    target_count: u64,
    concurrency: usize,
) -> Result<BenchmarkResult, Box<dyn std::error::Error>> {
    let per_stream = target_count / concurrency as u64;
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
                .get_stream_ids(IdStreamRequest { count: per_stream })
                .await?
                .into_inner();

            // let mut machine_id_counts = HashMap::new();
            // let mut received = HashSet::with_capacity(target_count as usize);
            let mut received = 0;
            while let Some(resp) = TokioStreamExt::next(&mut stream).await {
                let raw = resp?.packed_ids;
                let bytes = raw.as_ref();

                assert_eq!(bytes.len() % 8, 0, "Corrupt chunk: not a multiple of 8");

                for chunk in bytes.chunks_exact(8) {
                    let raw_id = u64::from_le_bytes(chunk.try_into().unwrap());
                    let _id = SnowflakeTwitterId::from_raw(raw_id);
                    received += 1;
                }
            }

            Ok::<usize, Box<dyn std::error::Error + Send + Sync>>(received)
        }));
    }

    // Collect all results
    let mut total_received = 0;
    while let Some(res) = FuturesStreamExt::next(&mut tasks).await {
        let res = res?;
        let d = res.unwrap();
        total_received += d;
    }

    let duration = start.elapsed();

    Ok(BenchmarkResult {
        label: format!("Parallel stream x{}", concurrency),
        count: total_received,
        duration,
    })
}
