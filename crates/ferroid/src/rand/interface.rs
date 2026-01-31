/// A trait for random sources that return random integers.
///
/// This abstraction allows you to plug in a real random source or a mocked
/// random source in tests.
///
/// The random type `T` is generic (typically `u64` or `u128`)
///
/// # Example
/// ```
/// use ferroid::rand::RandSource;
///
/// struct FixedRand;
/// impl RandSource<u64> for FixedRand {
///     fn rand(&self) -> u64 {
///         1234
///     }
/// }
///
/// let rng = FixedRand;
/// assert_eq!(rng.rand(), 1234);
/// ```
pub trait RandSource<T> {
    /// Returns a random integer.
    fn rand(&self) -> T;
}
