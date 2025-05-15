use criterion::{Criterion, Throughput, black_box, criterion_group, criterion_main};
use ferroid::{
    BasicSnowflakeGenerator, MonotonicClock, Snowflake, SnowflakeGenerator, SnowflakeTwitterId,
    TimeSource,
};
use ferroid_army::Army;
use std::time::Instant;

// Total number of IDs to generate per benchmark iteration
const TOTAL_IDS: usize = 4096 * 256; // Enough to simulate at least 256 Pending cycles

/// Benchmark a `SingleArmy` with the specified number of generators
fn bench_single_army<G, ID, T>(c: &mut Criterion, group_name: &str, generator_fn: impl Fn(u64) -> G)
where
    G: SnowflakeGenerator<ID, T>,
    ID: Snowflake,
    T: TimeSource<ID::Ty>,
{
    let mut group = c.benchmark_group(group_name);

    for num_generators in [1, 2, 4, 8, 16, 32, 64] {
        group.throughput(Throughput::Elements(TOTAL_IDS as u64));
        group.bench_function(
            format!("elems/{}/generators/{}", TOTAL_IDS, num_generators),
            |b| {
                b.iter_custom(|iters| {
                    let start = Instant::now();

                    for _ in 0..iters {
                        let generators: Vec<_> = (0..num_generators)
                            .map(|machine_id| generator_fn(machine_id))
                            .collect();
                        let mut army = Army::new(generators);

                        for _ in 0..TOTAL_IDS {
                            black_box(army.next_id());
                        }
                    }

                    start.elapsed()
                });
            },
        );
    }

    group.finish();
}

/// Run benchmark with various generator counts
fn benchmark_mono_sequential_army_basic(c: &mut Criterion) {
    bench_single_army::<_, SnowflakeTwitterId, _>(c, "mono/sequential/army/basic", |machine_id| {
        BasicSnowflakeGenerator::new(machine_id, MonotonicClock::default())
    })
}

criterion_group!(benches, benchmark_mono_sequential_army_basic);
criterion_main!(benches);
