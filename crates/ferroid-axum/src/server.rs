use axum::{Router, extract::State, routing::get};
use ferroid::{
    AtomicSnowflakeGenerator, MonotonicClock, Snowflake, SnowflakeGeneratorAsyncTokioExt,
    SnowflakeTwitterId,
};
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

// Shared state containing multiple generators for sharding
#[derive(Clone)]
struct AppState {
    generators: Arc<Vec<AtomicSnowflakeGenerator<SnowflakeTwitterId, MonotonicClock>>>,
    counter: Arc<AtomicUsize>,
}

impl AppState {
    fn new(num_shards: usize) -> Self {
        let clock: MonotonicClock = MonotonicClock::default();
        let mut generators = Vec::with_capacity(num_shards);
        for i in 0..num_shards {
            generators.push(AtomicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(
                i as u64,
                clock.clone(),
            ));
        }
        Self {
            generators: Arc::new(generators),
            counter: Arc::new(AtomicUsize::new(0)),
        }
    }

    async fn next_id(&self) -> String {
        // Round-robin selection of generator
        let index = self.counter.fetch_add(1, Ordering::Relaxed) % self.generators.len();
        let generator = &self.generators[index];

        match generator.try_next_id_async().await {
            Ok(id) => id.to_raw().to_string(),
            Err(e) => format!("error: {e:?}"),
        }
    }
}

async fn handler(State(state): State<AppState>) -> String {
    state.next_id().await
}

#[tokio::main]
async fn main() {
    // Use more shards to reduce contention - try 8 or 16
    let state = AppState::new(8);

    let app = Router::new().route("/", get(handler)).with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();

    println!("Server running on http://localhost:3000");
    axum::serve(listener, app).await.unwrap();
}
