//! gRPC ID generation service using ferroid and tonic. Provides single, batch,
//! and streaming ID generation via Snowflake-like IDs with cancellation
//! support.

use core::pin::Pin;
use ferroid::{
    BasicSnowflakeGenerator, IdGenStatus, MonotonicClock, Snowflake, SnowflakeTwitterId,
};
use futures::{StreamExt, stream::SelectAll};
use idgen::{
    IdStreamRequest, IdUnitResponseChunk,
    id_gen_server::{IdGen, IdGenServer},
};
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_stream::{Stream, wrappers::ReceiverStream};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status, codec::CompressionEncoding, transport::Server};

pub mod idgen {
    tonic::include_proto!("idgen");
}

#[derive(Debug)]
enum WorkRequest {
    Stream {
        count: usize,
        tx: mpsc::Sender<Result<IdUnitResponseChunk, Status>>,
        cancelled: Arc<CancellationToken>,
    },
}

struct IdService {
    workers: Arc<Vec<mpsc::Sender<WorkRequest>>>,
}

/// Number of worker tasks processing ID generation requests concurrently.
const NUM_WORKERS: usize = 128;
/// Buffer capacity for each worker's mpsc channel.
const DEFAULT_WORK_REQUEST_BUFF: usize = 256;
/// Default number of IDs generated per chunk.
const DEFAULT_CHUNK_SIZE: usize = 4096;
/// Default number of chunks buffered in the response stream.
const DEFAULT_STREAM_BUFFER_SIZE: usize = 8;

impl IdService {
    fn new(num_workers: usize) -> Self {
        let mut workers = Vec::with_capacity(num_workers);
        let clock = MonotonicClock::default();

        for worker_id in 0..num_workers {
            let (tx, mut rx) = mpsc::channel::<WorkRequest>(DEFAULT_WORK_REQUEST_BUFF);
            workers.push(tx);
            let generator = BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(
                worker_id as u64,
                clock.clone(),
            );

            tokio::spawn(async move {
                while let Some(work) = rx.recv().await {
                    match work {
                        WorkRequest::Stream {
                            count,
                            tx,
                            cancelled,
                        } => {
                            let mut chunk_buf = Vec::with_capacity(DEFAULT_CHUNK_SIZE * 8);
                            let mut generated = 0;

                            while generated < count {
                                match generator.try_next_id() {
                                    Ok(IdGenStatus::Ready { id }) => {
                                        chunk_buf.extend_from_slice(&id.to_raw().to_le_bytes());
                                        generated += 1;

                                        if chunk_buf.len() == DEFAULT_CHUNK_SIZE {
                                            if cancelled.is_cancelled() || tx.is_closed() {
                                                break;
                                            }

                                            let bytes =
                                                bytes::Bytes::from(std::mem::take(&mut chunk_buf));

                                            if tx
                                                .send(Ok(IdUnitResponseChunk { packed_ids: bytes }))
                                                .await
                                                .is_err()
                                            {
                                                println!("Failed to send res");
                                                break;
                                            }

                                            // Reuse the buffer by reserving again
                                            chunk_buf.reserve(DEFAULT_CHUNK_SIZE * 8);
                                        }
                                    }

                                    Ok(IdGenStatus::Pending { .. }) => {
                                        tokio::task::yield_now().await;
                                    }

                                    Err(e) => {
                                        if cancelled.is_cancelled() || tx.is_closed() {
                                            break;
                                        }

                                        if tx
                                            .send(Err(Status::internal(format!(
                                                "ID generation failed: {}",
                                                e
                                            ))))
                                            .await
                                            .is_err()
                                        {
                                            println!("Failed to send res");
                                            break;
                                        }
                                    }
                                }
                            }

                            if !chunk_buf.is_empty() && !cancelled.is_cancelled() && !tx.is_closed()
                            {
                                let bytes = bytes::Bytes::from(std::mem::take(&mut chunk_buf));

                                if tx
                                    .send(Ok(IdUnitResponseChunk { packed_ids: bytes }))
                                    .await
                                    .is_err()
                                {
                                    println!("Failed to send res");
                                }
                            }
                        }
                    }
                }
            });
        }

        Self {
            workers: Arc::new(workers),
        }
    }
}

#[tonic::async_trait]
impl IdGen for IdService {
    type GetStreamIdsStream =
        Pin<Box<dyn Stream<Item = Result<IdUnitResponseChunk, Status>> + Send>>;

    async fn get_stream_ids(
        &self,
        req: Request<IdStreamRequest>,
    ) -> Result<Response<Self::GetStreamIdsStream>, Status> {
        let cancellation_token = Arc::new(CancellationToken::new());
        let total_ids = req.get_ref().count as usize;
        let num_workers = self.workers.len();
        let ids_per_worker = if num_workers > 0 {
            total_ids / num_workers
        } else {
            0
        };
        let remainder = if num_workers > 0 {
            total_ids % num_workers
        } else {
            0
        };

        let mut streams = SelectAll::new();
        let mut rng = StdRng::from_rng(&mut rand::rng());

        for (i, tx) in self
            .workers
            .choose_multiple(&mut rng, num_workers)
            .enumerate()
        {
            let worker_count = if i < remainder {
                ids_per_worker + 1
            } else {
                ids_per_worker
            };
            if worker_count == 0 {
                continue;
            }

            let (resp_tx, resp_rx) = mpsc::channel(DEFAULT_STREAM_BUFFER_SIZE);
            tx.send(WorkRequest::Stream {
                count: worker_count,
                tx: resp_tx,
                cancelled: cancellation_token.clone(),
            })
            .await
            .map_err(|e| Status::unavailable(format!("Service overloaded: {}", e)))?;

            streams.push(ReceiverStream::new(resp_rx));
        }

        let cancel_future = Box::pin(async move {
            cancellation_token.cancelled().await;
        });

        let stream = streams.take_until(cancel_future);
        Ok(Response::new(Box::pin(stream)))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:50051".parse()?;
    println!(
        "Starting ID generation service with {} workers",
        NUM_WORKERS
    );

    let service = IdService::new(NUM_WORKERS);

    println!("gRPC ID service listening on {}", addr);

    Server::builder()
        .add_service(
            IdGenServer::new(service)
                .send_compressed(CompressionEncoding::Zstd)
                .accept_compressed(CompressionEncoding::Zstd),
        )
        .serve_with_shutdown(addr, async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C signal handler");
            println!("Shutdown signal received, terminating...");
        })
        .await?;

    Ok(())
}
