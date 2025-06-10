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

#[derive(Clone, Copy, Debug)]
enum Compression {
    None,
    Deflate,
    Gzip,
    Zstd,
}

impl Compression {
    fn as_label(&self) -> &'static str {
        match self {
            Compression::None => "none",
            Compression::Deflate => "deflate",
            Compression::Gzip => "gzip",
            Compression::Zstd => "zstd",
        }
    }
    fn as_tonic(&self) -> Option<CompressionEncoding> {
        match self {
            Compression::None => None,
            Compression::Deflate => Some(CompressionEncoding::Deflate),
            Compression::Gzip => Some(CompressionEncoding::Gzip),
            Compression::Zstd => Some(CompressionEncoding::Zstd),
        }
    }
}

#[derive(Debug, Clone)]
struct BenchParams {
    requests: usize,          // N (number of requests/tasks)
    ids_per_request: u64,     // how many IDs per request
    concurrency: usize,       // in-flight requests at a time
    compression: Compression, // which compression mode
}

#[tokio::main]
async fn main() -> Result<()> {
    let scenarios = [
        BenchParams {
            requests: 10,
            ids_per_request: 100,
            concurrency: 1,
            compression: Compression::None,
        },
        BenchParams {
            requests: 10,
            ids_per_request: 100,
            concurrency: 10,
            compression: Compression::Zstd,
        },
        BenchParams {
            requests: 100,
            ids_per_request: 1000,
            concurrency: 50,
            compression: Compression::Zstd,
        },
        BenchParams {
            requests: 100,
            ids_per_request: 1000,
            concurrency: 50,
            compression: Compression::Deflate,
        },
    ];

    let mut results = Vec::new();

    for params in &scenarios {
        results.push(run_bench(params).await?);
    }

    // === Summary Table ===
    println!("\n=== Benchmark Summary ===");
    println!(
        "{:<24} | {:>8} | {:>8} | {:>8} | {:>8} | {:>10} | {:>12}",
        "Mode", "Reqs", "IDs/Req", "Conc", "Total", "Time(ms)", "Throughput/s"
    );
    println!("{}", "-".repeat(80));
    for r in &results {
        r.report();
    }

    // Optional: print latency percentiles for each run (90th/99th/min/max)
    for r in &results {
        r.print_latency_stats();
    }
    Ok(())
}

#[derive(Debug)]
struct BenchmarkResult {
    params: BenchParams,
    total_received: usize,
    duration: Duration,
    per_req_durations: Vec<Duration>,
}

impl BenchmarkResult {
    fn throughput(&self) -> f64 {
        self.total_received as f64 / self.duration.as_secs_f64()
    }
    fn report(&self) {
        println!(
            "{:<24} | {:>8} | {:>8} | {:>8} | {:>8} | {:>10.2} | {:>12.2}",
            self.label(),
            self.params.requests,
            self.params.ids_per_request,
            self.params.concurrency,
            self.total_received,
            self.duration.as_secs_f64() * 1000.0,
            self.throughput()
        );
    }
    fn label(&self) -> String {
        format!(
            "{} x{} c{} [{}]",
            if self.params.concurrency == 1 {
                "sequential"
            } else {
                "parallel"
            },
            self.params.ids_per_request,
            self.params.concurrency,
            self.params.compression.as_label()
        )
    }
    fn print_latency_stats(&self) {
        if self.per_req_durations.is_empty() {
            return;
        }
        let mut durations_ms: Vec<f64> = self
            .per_req_durations
            .iter()
            .map(|d| d.as_secs_f64() * 1000.0)
            .collect();
        durations_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let min = durations_ms.first().copied().unwrap_or(0.0);
        let max = durations_ms.last().copied().unwrap_or(0.0);
        let p50 = percentile(&durations_ms, 50.0);
        let p90 = percentile(&durations_ms, 90.0);
        let p99 = percentile(&durations_ms, 99.0);
        println!(
            "[{}] latencies: min {:.2} ms | p50 {:.2} | p90 {:.2} | p99 {:.2} | max {:.2} ms",
            self.label(),
            min,
            p50,
            p90,
            p99,
            max
        );
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p / 100.0) * (sorted.len() - 1) as f64;
    let low = rank.floor() as usize;
    let high = rank.ceil() as usize;
    if low == high {
        sorted[low]
    } else {
        sorted[low] * (high as f64 - rank) + sorted[high] * (rank - low as f64)
    }
}

async fn run_bench(params: &BenchParams) -> Result<BenchmarkResult> {
    let BenchParams {
        requests,
        ids_per_request,
        concurrency,
        compression,
    } = *params;
    let start = Instant::now();

    let mut tasks = FuturesUnordered::new();
    let mut per_req_durations = Vec::with_capacity(requests);

    let channel = Channel::from_static("http://localhost:50051")
        .connect()
        .await?;

    // Distribute requests across workers
    for _ in 0..requests {
        let channel = channel.clone();
        let compression = compression;
        tasks.push(tokio::spawn(async move {
            let t0 = Instant::now();
            let mut client = IdGenClient::new(channel);
            if let Some(encoding) = compression.as_tonic() {
                client = client.send_compressed(encoding);
            }
            let mut stream = client
                .get_stream_ids(IdStreamRequest {
                    count: ids_per_request,
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
            let duration = t0.elapsed();
            Ok::<(usize, Duration), anyhow::Error>((received, duration))
        }));
        // Simple throttle to avoid spinning up more than `concurrency` at once
        if tasks.len() >= concurrency {
            let (_received, dur) = FutureStreamExt::next(&mut tasks).await.unwrap()??;
            per_req_durations.push(dur);
        }
    }
    // Drain remaining
    while let Some(res) = FutureStreamExt::next(&mut tasks).await {
        let (_received, dur) = res??;
        per_req_durations.push(dur);
    }
    let total_received = per_req_durations
        .iter()
        .map(|_| ids_per_request as usize)
        .sum();

    Ok(BenchmarkResult {
        params: params.clone(),
        total_received,
        duration: start.elapsed(),
        per_req_durations,
    })
}
