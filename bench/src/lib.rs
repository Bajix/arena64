use std::sync::atomic::{AtomicU64, Ordering};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn criterion_benchmark(criterion: &mut Criterion) {
    criterion.bench_function("AtomicU64/64", |bencher| {
        bencher.iter(|| {
            let occupancy = AtomicU64::new(0);

            let mut occupied: u64 = 0;

            loop {
                let least_significant_bit = !occupied & occupied.wrapping_add(1);

                if least_significant_bit.ne(&0) {
                    occupied = occupancy.fetch_or(least_significant_bit, Ordering::AcqRel)
                        | least_significant_bit;
                } else {
                    break;
                }
            }
        });
    });

    criterion.bench_function("Boxed64/64", |bencher| {
        use arena64::Boxed64;

        bencher.iter(|| {
            let slab: Boxed64<usize> = Boxed64::new();

            black_box(
                (0..64)
                    .map(|i| slab.get_uninit_slot().unwrap().insert(i))
                    .collect::<Vec<_>>(),
            );
        });
    });

    let mut alloc_bench = criterion.benchmark_group("Alloc");

    for n in 6..12 {
        let batch_size: usize = 1 << n;
        alloc_bench.bench_with_input(
            BenchmarkId::new("Box", batch_size),
            &batch_size,
            |b, batch_size| {
                b.iter(|| {
                    for i in 0..*batch_size {
                        black_box(Box::new(i));
                    }
                });
            },
        );

        alloc_bench.bench_with_input(
            BenchmarkId::new("Bump64", batch_size),
            &batch_size,
            |b, batch_size| {
                use arena64::Bump64;

                b.iter(|| {
                    let mut arena: Bump64<usize> = Bump64::new();
                    black_box((0..*batch_size).map(|i| arena.alloc(i)).collect::<Vec<_>>());
                });
            },
        );

        alloc_bench.bench_with_input(
            BenchmarkId::new("Arena64", batch_size),
            &batch_size,
            |b, batch_size| {
                use arena64::Arena64;

                b.iter(|| {
                    let arena: Arena64<usize> = Arena64::new();
                    black_box((0..*batch_size).map(|i| arena.alloc(i)).collect::<Vec<_>>());
                });
            },
        );
    }

    alloc_bench.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
