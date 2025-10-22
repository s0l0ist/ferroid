/// Internal helper to implement `fmt::Display` when the `base32` feature is enabled.
#[doc(hidden)]
#[macro_export]
macro_rules! cfg_base32 {
    ($($item:item)*) => {
        $(
            #[cfg(feature = "base32")]
            $item
        )*
    };
}
