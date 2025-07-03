use crate::RandSource;
use rand::{Rng, rng};

/// A `RandSource` that uses the thread-local RNG (`rand::thread_rng()`).
///
/// This RNG is fast, cryptographically secure (ChaCha-based), and automatically
/// reseeded periodically.
///
/// Each OS thread has its own RNG instance, so calls from multiple threads are
/// contention-free and safe. This type does **not** store the RNG itself; it
/// simply accesses the thread-local generator on each call.
///
/// ⚠️ NOTE: The underlying `ThreadRng` is not `Send` or `Sync`, meaning it
/// cannot be shared or moved across threads. However, since this type is a
/// zero-sized wrapper that does not store the RNG, it **is** thread-safe and
/// may be freely used across threads.
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
