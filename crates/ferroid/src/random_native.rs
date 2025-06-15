use crate::RandSource;
use rand::{Rng, rng};

/// A `RandSource` that uses the thread-local RNG (`rand::thread_rng()`).
///
/// This RNG is fast, cryptographically secure (ChaCha-based), and automatically
/// reseeded periodically.
///
/// Suitable for high-throughput, contention-free ID generation.
#[derive(Default, Clone)]
pub struct ThreadRandom;

impl RandSource<u64> for ThreadRandom {
    fn rand(&self) -> u64 {
        rng().random()
    }
}

impl RandSource<u128> for ThreadRandom {
    fn rand(&self) -> u128 {
        rng().random()
    }
}
