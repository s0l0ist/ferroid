use crate::RandSource;
use rand::{Rng, rng};

/// A `RandSource` that uses the thread-local RNG (`rand::thread_local()`).
///
/// This RNG is fast, cryptographically secure (ChaCha-based), and automatically
/// reseeded periodically.
///
/// Each OS thread has its own RNG instance, so calls from multiple threads are
/// contention-free and safe. This type does **not** store the RNG itself; it
/// simply accesses the thread-local generator on each call.
#[derive(Default, Clone, Debug)]
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
