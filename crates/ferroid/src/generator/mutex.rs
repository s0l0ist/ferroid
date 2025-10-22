#[cfg_attr(docsrs, doc(cfg(all(feature = "lock", not(feature = "parking-lot")))))]
#[cfg(all(feature = "lock", not(feature = "parking-lot")))]
pub use std::sync::{Mutex, MutexGuard, PoisonError};

#[cfg_attr(docsrs, doc(cfg(feature = "parking-lot")))]
#[cfg(feature = "parking-lot")]
pub use parking_lot::{Mutex, MutexGuard};
