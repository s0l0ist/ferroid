cfg_select! {
    feature = "parking-lot" => {
        pub(crate) use parking_lot::Mutex;

        /// Error returned by lock-based generators. Parking-lot locks cannot be poisoned.
        pub type LockError = core::convert::Infallible;
    }
    _ => {
        pub(crate) use std::sync::Mutex;

        /// Error returned by lock-based generators when a standard mutex is poisoned.
        pub type LockError = super::Error;
    }
}
