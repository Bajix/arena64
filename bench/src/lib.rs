use std::mem;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

fn criterion_benchmark(criterion: &mut Criterion) {
    criterion.bench_function("Fixed64/64", |bencher| {
        use arena64::heapless::Fixed64;

        bencher.iter(|| {
            let slab: Fixed64<usize> = Fixed64::new();

            for i in 0..64 {
                mem::forget(slab.get_uninit_slot().unwrap().insert(i));
            }
        });
    });

    criterion.bench_function("Boxed64/64", |bencher| {
        use arena64::boxed::Boxed64;

        bencher.iter(|| {
            let slab: Boxed64<usize> = Boxed64::new();

            for i in 0..64 {
                mem::forget(slab.get_uninit_slot().unwrap().insert(i));
            }
        });
    });

    for n in 6..17 {
        let batch_size: usize = 1 << n;

        criterion.bench_with_input(
            BenchmarkId::new("Arena64", batch_size),
            &batch_size,
            |b, batch_size| {
                use arena64::arena::{Arena64, Slot};

                b.iter(|| {
                    let arena: Arena64<usize> = Arena64::new();

                    let slots: Vec<Slot<usize>> =
                        (0..*batch_size).map(|i| arena.insert(i)).collect();

                    assert_eq!(slots, (0..*batch_size).collect::<Vec<usize>>());
                });
            },
        );
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
