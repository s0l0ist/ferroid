mod interface;
#[cfg(all(feature = "std", feature = "alloc", target_has_atomic = "64"))]
mod mono_clock;

pub use interface::*;
#[cfg(all(feature = "std", feature = "alloc", target_has_atomic = "64"))]
pub use mono_clock::*;
