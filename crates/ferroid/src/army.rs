use crate::{IdGenStatus, Result, Snowflake, SnowflakeGenerator, TimeSource};
use std::{cell::Cell, marker::PhantomData};

/// A cooperative, non-threadsafe wrapper around multiple [`SnowflakeGenerator`]
/// instances, distributing ID generation fairly across a pool of generators.
///
/// This scheduler rotates through generators in round-robin fashion. If a
/// generator returns [`IdGenStatus::Pending`], it yields and continues polling
/// the next one.
///
/// ## Features
///
/// - ❌ Not thread-safe
/// - ✅ Efficient single-threaded scheduling of many generators
/// - ✅ Resilient to temporary generator exhaustion
///
/// ## Recommended When
///
/// - You have many generators (e.g., per core or shard) and want to saturate
///   throughput
/// - You are writing a single-threaded benchmark or high-throughput coordinator
pub struct Army<G, ID, T>
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    generators: Vec<G>,
    num_generators: usize,
    next: Cell<usize>,
    _idt: PhantomData<(ID, T)>,
}

impl<G, ID, T> Army<G, ID, T>
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    /// Creates a new [`Army`] with a given vector of generators.
    ///
    /// Each generator will be polled cooperatively to balance ID generation.
    ///
    /// # Parameters
    /// - `generators`: A `Vec<G>` where each generator must implement
    ///   [`SnowflakeGenerator`].
    ///
    /// # Returns
    /// A new [`Army`] instance, ready to generate IDs by rotating across
    /// generators.
    ///
    /// # Panics
    /// Panics if the `generators` vector is empty.
    ///
    /// # Example
    /// ```
    /// use ferroid::{Army, BasicSnowflakeGenerator, SnowflakeTwitterId, MonotonicClock, TimeSource};
    ///
    /// let clock = MonotonicClock::default();
    /// let generators = (0..4)
    ///     .map(|id| BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(id, clock.clone()))
    ///     .collect();
    ///
    /// let army = Army::new(generators);
    /// let id = army.next_id();
    /// ```
    pub fn new(generators: Vec<G>) -> Self {
        let length = generators.len();
        assert!(length > 0, "must have at least 1 generator");
        Self {
            num_generators: length,
            next: Cell::new(0),
            generators,
            _idt: PhantomData,
        }
    }

    /// Attempts to generate the next ID by polling underlying generators in
    /// round-robin order.
    ///
    /// # Panics
    /// This method currently has no fallible code paths, but may panic if an
    /// internal error occurs in future implementations. For explicitly fallible
    /// behavior, use [`Self::try_next_id`] instead.
    pub fn next_id(&self) -> ID {
        self.try_next_id().unwrap()
    }

    /// Attempts to generate the next ID by polling underlying generators in
    /// round-robin order.
    ///
    /// This method continuously rotates through each generator until one yields
    /// a valid ID. If a generator returns [`IdGenStatus::Pending`], it is
    /// skipped temporarily and retried on a future poll.
    ///
    /// # Returns
    /// - `Ok(id)`: When a generator yields a valid ID.
    /// - `Err(e)`: If a generator fails unexpectedly.
    ///
    /// # Fairness
    /// This scheduler guarantees **fairness** by rotating through each
    /// generator in turn, and immediately moving on if one becomes unavailable
    /// (e.g., due to exhausted sequence space).
    ///
    /// # Example
    /// ```
    /// use ferroid::{Army, BasicSnowflakeGenerator, SnowflakeTwitterId, MonotonicClock, TimeSource};
    ///
    /// let clock = MonotonicClock::default();
    ///
    /// // We use linear machine IDs here (0..4),
    /// // but you can assign IDs using any sharding or partitioning scheme.
    /// let generators = (0..=4)
    ///     .map(|machine_id| BasicSnowflakeGenerator::<SnowflakeTwitterId, _>::new(machine_id, clock.clone()))
    ///     .collect();
    ///
    /// let army = Army::new(generators);
    ///
    /// for i in 0..4 {
    ///     let id = army.try_next_id().unwrap();
    ///
    ///     // Each generator should be called in round-robin order.
    ///     assert_eq!(id.machine_id(), i);
    ///
    ///     // We're not exhausting the sequence (limit = 4096),
    ///     // and the clock shouldn't have advanced by 1 ms in this loop.
    ///     assert_eq!(id.sequence(), 0);
    /// }
    /// ```
    pub fn try_next_id(&self) -> Result<ID> {
        loop {
            match self.generators[self.next.get()].try_next_id()? {
                IdGenStatus::Ready { id } => {
                    self.next.set((self.next.get() + 1) % self.num_generators);
                    return Ok(id);
                }
                IdGenStatus::Pending { .. } => {
                    self.next.set((self.next.get() + 1) % self.num_generators);
                    std::thread::yield_now();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasicSnowflakeGenerator, MonotonicClock, SnowflakeTwitterId};
    use core::fmt;
    use std::collections::HashMap;
    use std::collections::HashSet;

    const TOTAL_IDS: usize = 4096 * 32; // Enough to simulate at least 32 Pending cycles

    fn test_single_army<G, ID, T>(generator_fn: impl Fn(u64, T) -> G, clock_factory: impl Fn() -> T)
    where
        G: SnowflakeGenerator<ID, T>,
        ID: Snowflake + fmt::Debug,
        T: TimeSource<ID::Ty> + Clone,
    {
        for num_generators in [1, 2, 4, 8, 16, 32] {
            let clock = clock_factory(); // create one shared clock
            let generators: Vec<_> = (0..num_generators)
                .map(|machine_id| generator_fn(machine_id, clock.clone()))
                .collect();
            let army = Army::new(generators);
            let mut histogram: HashMap<ID::Ty, usize> = HashMap::with_capacity(TOTAL_IDS);
            let mut seen_ids = HashSet::with_capacity(TOTAL_IDS);

            for _ in 0..TOTAL_IDS {
                let id = army.next_id();
                assert!(seen_ids.insert(id), "Duplicate ID detected: {:?}", id);
                *histogram.entry(id.machine_id()).or_insert(0) += 1;
            }
            assert_eq!(
                histogram.values().copied().sum::<usize>(),
                TOTAL_IDS,
                "Expected {} unique IDs",
                TOTAL_IDS
            );

            let mut bins: Vec<_> = histogram.into_iter().collect();
            bins.sort_by_key(|(mach, _)| *mach);

            println!("Histogram of generated IDs by machine_id:");
            for (mach, count) in bins {
                println!("  machine_id {:>3}: {}", mach, count);
            }
        }
    }

    /// Run benchmark with various generator counts
    #[test]
    fn test_mono_sequential_army_basic() {
        test_single_army::<_, SnowflakeTwitterId, _>(
            |machine_id, clock| BasicSnowflakeGenerator::new(machine_id, clock),
            || MonotonicClock::default(),
        )
    }
}
