use core::{fmt, hint::black_box};
use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use ferroid::Snowflake;
use ferroid_tonic_core::{
    proto::{StreamIdsRequest, id_generator_client::IdGeneratorClient},
    types::{SNOWFLAKE_ID_SIZE, SnowflakeId, SnowflakeIdTy},
};

use futures::stream::FuturesUnordered;
use std::{
    net::TcpStream,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use tokio::runtime::Builder;
use tokio_stream::StreamExt;
use tonic::{
    codec::CompressionEncoding,
    transport::{Channel, Uri},
};

#[derive(Clone, Copy, Debug)]
enum Compression {
    None,
    Deflate,
    Gzip,
    Zstd,
}

impl fmt::Display for Compression {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Compression::None => write!(f, "none"),
            Compression::Deflate => write!(f, "deflate"),
            Compression::Gzip => write!(f, "gzip"),
            Compression::Zstd => write!(f, "zstd"),
        }
    }
}

impl From<Compression> for Option<CompressionEncoding> {
    fn from(value: Compression) -> Self {
        match value {
            Compression::None => None,
            Compression::Deflate => Some(CompressionEncoding::Deflate),
            Compression::Gzip => Some(CompressionEncoding::Gzip),
            Compression::Zstd => Some(CompressionEncoding::Zstd),
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct GrpcBenchParams {
    ids_per_request: u64,
    concurrency: usize,
    compression: Compression,
}

fn grpc_bench(c: &mut Criterion) {
    let uri = Uri::try_from("http://0.0.0.0:50051").expect("Invalid URI");
    // Start the server. This may require a full compilation so set the timeout
    // high. Adjust features and CLI args to the server as necessary.
    let mut server = Command::new("cargo")
        .args([
            "run",
            "--bin",
            "tonic-server",
            "--release",
            "--features",
            "tracing",
            "--",
            "--num-workers",
            "128",
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to start tonic-server");
    wait_for_port(uri.authority().expect("missing authority").as_str(), 300);

    let ids_per_request_cases = [10_000, 100_000, 1_000_000];
    let concurrency_cases = [1, 2, 4, 8, 16, 32];
    let compression_cases = [
        Compression::None,
        Compression::Zstd,
        Compression::Gzip,
        Compression::Deflate,
    ];

    // Generate cartesian product of all param combinations
    let mut cases = Vec::new();
    for &ids_per_request in &ids_per_request_cases {
        for &concurrency in &concurrency_cases {
            for &compression in &compression_cases {
                cases.push(GrpcBenchParams {
                    ids_per_request,
                    concurrency,
                    compression,
                });
            }
        }
    }
    let rt = Builder::new_multi_thread().enable_all().build().unwrap();

    for params in &cases {
        let mut group = c.benchmark_group("grpc/get_stream_ids");
        group.throughput(Throughput::Elements(
            params.ids_per_request * params.concurrency as u64,
        ));

        group.bench_function(
            format!(
                "elems/{}/conc/{}/comp/{}",
                params.ids_per_request, params.concurrency, params.compression,
            ),
            |b| {
                b.to_async(&rt).iter_custom(|iters| {
                    let uri = uri.clone();
                    async move {
                        let channel = Channel::builder(uri)
                            .connect()
                            .await
                            .expect("Failed to connect to server");

                        let start = Instant::now();

                        for _ in 0..iters {
                            run_grpc_id_bench(&channel, params).await;
                        }

                        start.elapsed()
                    }
                });
            },
        );

        group.finish();
    }

    if server.kill().is_err() {
        eprintln!("failed to kill server");
    }
}

async fn run_grpc_id_bench(channel: &Channel, params: &GrpcBenchParams) {
    let concurrent_requests = params.concurrency;
    let mut tasks = FuturesUnordered::new();

    for _ in 0..concurrent_requests {
        let channel = channel.clone();
        let compression = params.compression;
        let ids_per_request = params.ids_per_request;

        tasks.push(tokio::spawn(async move {
            let mut client = IdGeneratorClient::new(channel);
            if let Some(encoding) = compression.into() {
                client = client.accept_compressed(encoding).send_compressed(encoding)
            }

            let mut stream = client
                .stream_ids(StreamIdsRequest {
                    count: ids_per_request,
                })
                .await
                .expect("stream call failed")
                .into_inner();

            while let Some(resp) = stream.next().await {
                let raw = resp.expect("resp").packed_ids;
                let bytes = raw.as_ref();
                assert_eq!(
                    bytes.len() % SNOWFLAKE_ID_SIZE,
                    0,
                    "Corrupt chunk: not a multiple of SNOWFLAKE_ID_SIZE"
                );
                for chunk in bytes.chunks_exact(SNOWFLAKE_ID_SIZE) {
                    let raw_id = SnowflakeIdTy::from_le_bytes(chunk.try_into().unwrap());
                    let _id = SnowflakeId::from_raw(raw_id);
                    black_box(_id);
                }
            }
        }));
    }

    // Wait for all tasks to complete
    while let Some(res) = tasks.next().await {
        res.unwrap();
    }
}

pub fn wait_for_port(addr: &str, timeout_secs: u64) {
    let start = Instant::now();
    while start.elapsed().as_secs() < timeout_secs {
        if TcpStream::connect(addr).is_ok() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    panic!("Server did not start listening on {}", addr);
}

criterion_group!(grpc_benches, grpc_bench);
criterion_main!(grpc_benches);
