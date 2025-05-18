use ferroid::{IdGenStatus, Result, Snowflake, SnowflakeGenerator, TimeSource};
use std::collections::VecDeque;
use std::marker::PhantomData;

pub struct Army<G, ID, T>
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    generators: Vec<G>,
    ready: VecDeque<usize>,
    pending: VecDeque<usize>,
    _id: PhantomData<ID>,
    _t: PhantomData<T>,
}

impl<G, ID, T> Army<G, ID, T>
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    pub fn new(generators: Vec<G>) -> Self {
        Self {
            pending: VecDeque::with_capacity(generators.len()),
            ready: (0..generators.len()).collect(),
            generators,
            _id: PhantomData,
            _t: PhantomData,
        }
    }

    pub fn next_id(&mut self) -> ID {
        self.try_next_id().unwrap()
    }

    pub fn try_next_id(&mut self) -> Result<ID> {
        // TODO: make this fair scheduling.
        loop {
            let len = self.ready.len();

            for _ in 0..len {
                let idx = self
                    .ready
                    .pop_front()
                    .expect("ready queue empty during poll");

                match self.generators[idx].try_next() {
                    Ok(IdGenStatus::Ready { id }) => {
                        self.ready.push_back(idx);
                        return Ok(id);
                    }
                    Ok(IdGenStatus::Pending { .. }) => {
                        self.pending.push_back(idx);
                    }
                    Err(e) => {
                        self.pending.push_back(idx);
                        return Err(e);
                    }
                }
            }

            std::mem::swap(&mut self.ready, &mut self.pending);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt;
    use ferroid::BasicSnowflakeGenerator;
    use ferroid::MonotonicClock;
    use ferroid::SnowflakeTwitterId;
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::sync::Mutex;

    const TOTAL_IDS: usize = 4096 * 5; // Enough to simulate at least 256 Pending cycles

    /// Benchmark a `SingleArmy` with the specified number of generators
    fn bench_single_army<G, ID, T>(
        generator_fn: impl Fn(u64, T) -> G,
        clock_factory: impl Fn() -> T,
    ) where
        G: SnowflakeGenerator<ID, T>,
        ID: Snowflake + fmt::Debug,
        T: TimeSource<ID::Ty> + Clone,
    {
        for num_generators in [128] {
            let clock = clock_factory(); // create one shared clock
            let generators: Vec<_> = (0..num_generators)
                .map(|machine_id| generator_fn(machine_id, clock.clone()))
                .collect();
            let mut army = Army::new(generators);
            let seen_ids = Arc::new(Mutex::new(HashSet::with_capacity(TOTAL_IDS)));

            for _ in 0..TOTAL_IDS {
                let id = army.next_id();
                let mut set = seen_ids.lock().unwrap();
                assert!(set.insert(id));
            }
            let mut sorted: Vec<_> = seen_ids.lock().unwrap().clone().into_iter().collect();
            sorted.sort();
            for id in &sorted {
                println!(
                    "time: {}, mach: {}, seq: {}",
                    id.timestamp(),
                    id.machine_id(),
                    id.sequence()
                );
            }

            assert_eq!(sorted.len(), TOTAL_IDS, "Expected {} unique IDs", TOTAL_IDS);
        }
    }

    /// Run benchmark with various generator counts
    #[test]
    fn test_mono_sequential_army_basic() {
        bench_single_army::<_, SnowflakeTwitterId, _>(
            |machine_id, clock| BasicSnowflakeGenerator::new(machine_id, clock),
            || MonotonicClock::default(),
        )
    }
}
