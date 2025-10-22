#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
pub use std::sync::{Mutex, MutexGuard, PoisonError};

#[cfg(feature = "parking-lot")]
pub use parking_lot::{Mutex, MutexGuard};
