use std::{
    mem,
    sync::atomic::{AtomicU64, Ordering},
};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn criterion_benchmark(criterion: &mut Criterion) {
    criterion.bench_function("Box::new", |bencher| {
        bencher.iter(|| {
            black_box(|| {
                drop(Box::new(1));
            })();
        });
    });

    criterion.bench_function("Box/64", |bencher| {
        bencher.iter(|| {
            black_box(|| {
                for i in 0..64 {
                    let _ = Box::new(i);
                }
            })();
        });
    });

    criterion.bench_function("AtomicU64::fetch_or", |bencher| {
        bencher.iter(|| {
            black_box(|| {
                AtomicU64::new(0).fetch_or(1, Ordering::AcqRel);
            })();
        });
    });

    criterion.bench_function("AtomicU64/64", |bencher| {
        bencher.iter(|| {
            black_box(|| {
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
            })();
        });
    });

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
            BenchmarkId::new("Bump64", batch_size),
            &batch_size,
            |b, batch_size| {
                use arena64::arena::{Bump64, Slot};

                b.iter(|| {
                    let mut arena: Bump64<usize> = Bump64::new();

                    let slots: Vec<Slot<usize>> =
                        (0..*batch_size).map(|i| arena.insert(i)).collect();

                    assert_eq!(slots, (0..*batch_size).collect::<Vec<usize>>());
                });
            },
        );

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
