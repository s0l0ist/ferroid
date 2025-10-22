#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
pub type Mutex = std::sync::Mutex;
#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
pub type MutexGuard = std::sync::MutexGuard;
#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
pub type PoisonError = std::sync::PoisonError;

#[cfg(feature = "parking-lot")]
pub use parking_lot::{Mutex, MutexGuard};
