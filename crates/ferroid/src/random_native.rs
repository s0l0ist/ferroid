use crate::RandSource;
use rand::{Rng, rngs::ThreadRng};

/// A `RandSource` that uses the thread-local RNG (`rand::thread_rng()`).
///
/// This RNG is fast, cryptographically secure (ChaCha-based), and automatically
/// reseeded periodically.
///
/// Suitable for high-throughput, contention-free ID generation.
#[derive(Default, Clone)]
pub struct ThreadRandom {
    rng: ThreadRng,
}

impl RandSource<u64> for ThreadRandom {
    fn rand(&mut self) -> u64 {
        self.rng.random()
    }
}

impl RandSource<u128> for ThreadRandom {
    fn rand(&mut self) -> u128 {
        self.rng.random()
    }
}
