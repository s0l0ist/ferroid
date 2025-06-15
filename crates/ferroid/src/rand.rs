pub trait RandSource<T> {
    /// Returns random bytes.
    fn rand(&mut self) -> T;
}
