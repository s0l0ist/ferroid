//! Definition-site feature gates for items emitted by exported ID macros.
//!
//! Each gate checks `ferroid`'s features instead of the consuming crate's.
//!
//! Keep these definitions outside macro expansions: Rust rejects absolute paths
//! to macro-expanded exports when they are invoked inside the defining crate.

#[cfg(feature = "base32")]
#[doc(hidden)]
#[macro_export]
macro_rules! cfg_base32 {
    ($($item:item)*) => { $($item)* };
}

#[cfg(not(feature = "base32"))]
#[doc(hidden)]
#[macro_export]
macro_rules! cfg_base32 {
    ($($item:item)*) => {};
}

#[cfg(feature = "std")]
#[doc(hidden)]
#[macro_export]
macro_rules! cfg_std {
    ($($item:item)*) => { $($item)* };
}

#[cfg(not(feature = "std"))]
#[doc(hidden)]
#[macro_export]
macro_rules! cfg_std {
    ($($item:item)*) => {};
}

#[cfg(feature = "alloc")]
#[doc(hidden)]
#[macro_export]
macro_rules! cfg_alloc {
    ($($item:item)*) => { $($item)* };
}

#[cfg(not(feature = "alloc"))]
#[doc(hidden)]
#[macro_export]
macro_rules! cfg_alloc {
    ($($item:item)*) => {};
}

#[cfg(feature = "thread-rng")]
#[doc(hidden)]
#[macro_export]
macro_rules! cfg_thread_rng {
    ($($item:item)*) => { $($item)* };
}

#[cfg(not(feature = "thread-rng"))]
#[doc(hidden)]
#[macro_export]
macro_rules! cfg_thread_rng {
    ($($item:item)*) => {};
}
