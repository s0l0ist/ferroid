/// Internal helper to implement `fmt::Display` when the `base32` feature is enabled.
#[doc(hidden)]
#[cfg(feature = "base32")]
#[macro_export]
macro_rules! cfg_base32 {
    ($($item:item)*) => { $($item)* };
}

#[doc(hidden)]
#[cfg(not(feature = "base32"))]
#[macro_export]
macro_rules! cfg_base32 {
    ($($item:item)*) => {};
}
