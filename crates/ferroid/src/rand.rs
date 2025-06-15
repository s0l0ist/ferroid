pub trait RandSource<T> {
    /// Returns random bytes.
    fn rand(&self) -> T;
}
