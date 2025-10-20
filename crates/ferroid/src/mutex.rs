#[cfg(feature = "parking-lot")]
pub use parking_lot::{Mutex, MutexGuard};
#[cfg(not(feature = "parking-lot"))]
pub use std::sync::{Mutex, MutexGuard, PoisonError};
