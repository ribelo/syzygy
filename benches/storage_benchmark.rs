#![allow(dead_code, clippy::clone_on_ref_ptr, unused_variables, unused_imports, clippy::let_and_return, clippy::format_in_format_args, clippy::type_complexity, clippy::duplicated_attributes, clippy::unnecessary_wraps, clippy::unused_self, clippy::derivable_impls, clippy::match_same_arms, clippy::cast_possible_truncation, clippy::items_after_statements)]
//! Storage Performance Benchmarks
//!
//! Measures critical storage performance paths:
//! 1. Type lookup traversal performance (current chain vs alternatives)
//! 2. Multi-model extraction patterns
//! 3. Cache-friendly vs cache-unfriendly layouts
//! 4. Comparison against HashMap<TypeId, Box<dyn Any>>

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::any::{Any, TypeId};
use std::collections::HashMap;
use syzygy::storage::{BulkExtract, EmptyStorage, Storage};

// ============================================================================
// Test Models for Benchmarking
// ============================================================================

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Model1 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model2 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model3 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model4 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model5 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model6 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model7 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model8 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model9 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model10 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model11 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model12 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model13 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model14 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model15 {
    value: u64,
    data: [u8; 64],
}

#[derive(Debug, Clone)]
struct Model16 {
    value: u64,
    data: [u8; 64],
}

impl Default for Model1 {
    fn default() -> Self {
        Self {
            value: 1,
            data: [1; 64],
        }
    }
}
impl Default for Model2 {
    fn default() -> Self {
        Self {
            value: 2,
            data: [2; 64],
        }
    }
}
impl Default for Model3 {
    fn default() -> Self {
        Self {
            value: 3,
            data: [3; 64],
        }
    }
}
impl Default for Model4 {
    fn default() -> Self {
        Self {
            value: 4,
            data: [4; 64],
        }
    }
}
impl Default for Model5 {
    fn default() -> Self {
        Self {
            value: 5,
            data: [5; 64],
        }
    }
}
impl Default for Model6 {
    fn default() -> Self {
        Self {
            value: 6,
            data: [6; 64],
        }
    }
}
impl Default for Model7 {
    fn default() -> Self {
        Self {
            value: 7,
            data: [7; 64],
        }
    }
}
impl Default for Model8 {
    fn default() -> Self {
        Self {
            value: 8,
            data: [8; 64],
        }
    }
}
impl Default for Model9 {
    fn default() -> Self {
        Self {
            value: 9,
            data: [9; 64],
        }
    }
}
impl Default for Model10 {
    fn default() -> Self {
        Self {
            value: 10,
            data: [10; 64],
        }
    }
}
impl Default for Model11 {
    fn default() -> Self {
        Self {
            value: 11,
            data: [11; 64],
        }
    }
}
impl Default for Model12 {
    fn default() -> Self {
        Self {
            value: 12,
            data: [12; 64],
        }
    }
}
impl Default for Model13 {
    fn default() -> Self {
        Self {
            value: 13,
            data: [13; 64],
        }
    }
}
impl Default for Model14 {
    fn default() -> Self {
        Self {
            value: 14,
            data: [14; 64],
        }
    }
}
impl Default for Model15 {
    fn default() -> Self {
        Self {
            value: 15,
            data: [15; 64],
        }
    }
}
impl Default for Model16 {
    fn default() -> Self {
        Self {
            value: 16,
            data: [16; 64],
        }
    }
}

// ============================================================================
// Storage Chain Creation Helpers
// ============================================================================

type SmallChain = Storage<Model1, Storage<Model2, EmptyStorage>>;
type MediumChain = Storage<Model1, Storage<Model2, Storage<Model3, Storage<Model4, EmptyStorage>>>>;
type LargeChain = Storage<
    Model1,
    Storage<
        Model2,
        Storage<
            Model3,
            Storage<
                Model4,
                Storage<Model5, Storage<Model6, Storage<Model7, Storage<Model8, EmptyStorage>>>>,
            >,
        >,
    >,
>;

type ExtraLargeChain = Storage<
    Model1,
    Storage<
        Model2,
        Storage<
            Model3,
            Storage<
                Model4,
                Storage<
                    Model5,
                    Storage<
                        Model6,
                        Storage<
                            Model7,
                            Storage<
                                Model8,
                                Storage<
                                    Model9,
                                    Storage<
                                        Model10,
                                        Storage<
                                            Model11,
                                            Storage<
                                                Model12,
                                                Storage<
                                                    Model13,
                                                    Storage<
                                                        Model14,
                                                        Storage<
                                                            Model15,
                                                            Storage<Model16, EmptyStorage>,
                                                        >,
                                                    >,
                                                >,
                                            >,
                                        >,
                                    >,
                                >,
                            >,
                        >,
                    >,
                >,
            >,
        >,
    >,
>;

fn create_small_chain() -> SmallChain {
    EmptyStorage
        .with_model(Model2::default())
        .with_model(Model1::default())
}

fn create_medium_chain() -> MediumChain {
    EmptyStorage
        .with_model(Model4::default())
        .with_model(Model3::default())
        .with_model(Model2::default())
        .with_model(Model1::default())
}

fn create_large_chain() -> LargeChain {
    EmptyStorage
        .with_model(Model8::default())
        .with_model(Model7::default())
        .with_model(Model6::default())
        .with_model(Model5::default())
        .with_model(Model4::default())
        .with_model(Model3::default())
        .with_model(Model2::default())
        .with_model(Model1::default())
}

fn create_extra_large_chain() -> ExtraLargeChain {
    EmptyStorage
        .with_model(Model16::default())
        .with_model(Model15::default())
        .with_model(Model14::default())
        .with_model(Model13::default())
        .with_model(Model12::default())
        .with_model(Model11::default())
        .with_model(Model10::default())
        .with_model(Model9::default())
        .with_model(Model8::default())
        .with_model(Model7::default())
        .with_model(Model6::default())
        .with_model(Model5::default())
        .with_model(Model4::default())
        .with_model(Model3::default())
        .with_model(Model2::default())
        .with_model(Model1::default())
}

// ============================================================================
// HashMap Alternative Implementation
// ============================================================================

struct HashMapStorage {
    map: HashMap<TypeId, Box<dyn Any>>,
}

impl HashMapStorage {
    fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    fn insert<T: Any + Clone>(&mut self, value: T) {
        self.map.insert(TypeId::of::<T>(), Box::new(value));
    }

    fn get<T: Any + Clone>(&self) -> Option<&T> {
        self.map.get(&TypeId::of::<T>())?.downcast_ref::<T>()
    }

    fn get_mut<T: Any + Clone>(&mut self) -> Option<&mut T> {
        self.map.get_mut(&TypeId::of::<T>())?.downcast_mut::<T>()
    }
}

fn create_hashmap_storage() -> HashMapStorage {
    let mut storage = HashMapStorage::new();
    storage.insert(Model1::default());
    storage.insert(Model2::default());
    storage.insert(Model3::default());
    storage.insert(Model4::default());
    storage.insert(Model5::default());
    storage.insert(Model6::default());
    storage.insert(Model7::default());
    storage.insert(Model8::default());
    storage
}

// ============================================================================
// Single Type Access Benchmarks
// ============================================================================

fn bench_single_type_access(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_type_access");

    let small_chain = create_small_chain();
    let medium_chain = create_medium_chain();
    let large_chain = create_large_chain();
    let mut hashmap_storage = create_hashmap_storage();

    // Head access (best case for chains)
    group.bench_function("chain_small_head", |b| {
        b.iter(|| {
            let model: &Model1 = black_box(&small_chain).get();
            black_box(model.value)
        });
    });

    group.bench_function("chain_medium_head", |b| {
        b.iter(|| {
            let model: &Model1 = black_box(&medium_chain).get();
            black_box(model.value)
        });
    });

    group.bench_function("chain_large_head", |b| {
        b.iter(|| {
            let model: &Model1 = black_box(&large_chain).get();
            black_box(model.value)
        });
    });

    // Tail access (worst case for chains)
    group.bench_function("chain_small_tail", |b| {
        b.iter(|| {
            let model: &Model2 = black_box(&small_chain).get();
            black_box(model.value)
        });
    });

    group.bench_function("chain_medium_tail", |b| {
        b.iter(|| {
            let model: &Model4 = black_box(&medium_chain).get();
            black_box(model.value)
        });
    });

    group.bench_function("chain_large_tail", |b| {
        b.iter(|| {
            let model: &Model8 = black_box(&large_chain).get();
            black_box(model.value)
        });
    });

    // HashMap comparison
    group.bench_function("hashmap_access", |b| {
        b.iter(|| {
            let model: &Model1 = black_box(&hashmap_storage).get().unwrap();
            black_box(model.value)
        });
    });

    group.bench_function("hashmap_access_mut", |b| {
        b.iter(|| {
            let model: &mut Model1 = black_box(&mut hashmap_storage).get_mut().unwrap();
            model.value += 1;
            black_box(model.value)
        });
    });

    group.finish();
}

// ============================================================================
// Multi-Model Extraction Benchmarks
// ============================================================================

fn bench_multi_model_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_model_extraction");

    let small_chain = create_small_chain();
    let medium_chain = create_medium_chain();
    let large_chain = create_large_chain();
    let hashmap_storage = create_hashmap_storage();

    // Extract all models from chain (current N-traversal approach)
    group.bench_function("chain_small_extract_all", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&small_chain).get();
            let m2: &Model2 = black_box(&small_chain).get();
            (black_box(m1.value), black_box(m2.value))
        });
    });

    group.bench_function("chain_medium_extract_all", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&medium_chain).get();
            let m2: &Model2 = black_box(&medium_chain).get();
            let m3: &Model3 = black_box(&medium_chain).get();
            let m4: &Model4 = black_box(&medium_chain).get();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
                black_box(m4.value),
            )
        });
    });

    group.bench_function("chain_large_extract_all", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&large_chain).get();
            let m2: &Model2 = black_box(&large_chain).get();
            let m3: &Model3 = black_box(&large_chain).get();
            let m4: &Model4 = black_box(&large_chain).get();
            let m5: &Model5 = black_box(&large_chain).get();
            let m6: &Model6 = black_box(&large_chain).get();
            let m7: &Model7 = black_box(&large_chain).get();
            let m8: &Model8 = black_box(&large_chain).get();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
                black_box(m4.value),
                black_box(m5.value),
                black_box(m6.value),
                black_box(m7.value),
                black_box(m8.value),
            )
        });
    });

    // HashMap all-access comparison
    group.bench_function("hashmap_extract_all", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&hashmap_storage).get().unwrap();
            let m2: &Model2 = black_box(&hashmap_storage).get().unwrap();
            let m3: &Model3 = black_box(&hashmap_storage).get().unwrap();
            let m4: &Model4 = black_box(&hashmap_storage).get().unwrap();
            let m5: &Model5 = black_box(&hashmap_storage).get().unwrap();
            let m6: &Model6 = black_box(&hashmap_storage).get().unwrap();
            let m7: &Model7 = black_box(&hashmap_storage).get().unwrap();
            let m8: &Model8 = black_box(&hashmap_storage).get().unwrap();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
                black_box(m4.value),
                black_box(m5.value),
                black_box(m6.value),
                black_box(m7.value),
                black_box(m8.value),
            )
        });
    });

    group.finish();
}

// ============================================================================
// Storage Creation Benchmarks
// ============================================================================

fn bench_storage_creation(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_creation");

    group.bench_function("chain_creation_small", |b| {
        b.iter(|| black_box(create_small_chain()));
    });

    group.bench_function("chain_creation_medium", |b| {
        b.iter(|| black_box(create_medium_chain()));
    });

    group.bench_function("chain_creation_large", |b| {
        b.iter(|| black_box(create_large_chain()));
    });

    group.bench_function("hashmap_creation", |b| {
        b.iter(|| black_box(create_hashmap_storage()));
    });

    group.finish();
}

// ============================================================================
// Cache Performance Simulation
// ============================================================================

fn bench_cache_performance(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_performance");

    let large_chain = create_large_chain();

    // Simulate realistic access patterns
    group.bench_function("chain_sequential_access", |b| {
        b.iter(|| {
            // Access in order of chain structure (cache-friendly)
            let m1: &Model1 = black_box(&large_chain).get();
            let m2: &Model2 = black_box(&large_chain).get();
            let m3: &Model3 = black_box(&large_chain).get();
            let m4: &Model4 = black_box(&large_chain).get();
            black_box((m1.value, m2.value, m3.value, m4.value))
        });
    });

    group.bench_function("chain_random_access", |b| {
        b.iter(|| {
            // Access in reverse order (potentially cache-unfriendly)
            let m4: &Model4 = black_box(&large_chain).get();
            let m1: &Model1 = black_box(&large_chain).get();
            let m3: &Model3 = black_box(&large_chain).get();
            let m2: &Model2 = black_box(&large_chain).get();
            black_box((m4.value, m1.value, m3.value, m2.value))
        });
    });

    group.finish();
}

// ============================================================================
// Bulk vs Individual Extraction Benchmarks
// ============================================================================

fn bench_bulk_vs_individual(c: &mut Criterion) {
    let mut group = c.benchmark_group("bulk_vs_individual");

    let medium_chain = create_medium_chain();
    let large_chain = create_large_chain();

    // Individual extraction (2 separate get() calls)
    group.bench_function("individual_two_get", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&medium_chain).get();
            let m2: &Model2 = black_box(&medium_chain).get();
            (black_box(m1.value), black_box(m2.value))
        });
    });

    // Bulk extraction using type-inferred extract_bulk() - 2-tuple
    group.bench_function("bulk_extract_two", |b| {
        b.iter(|| {
            let (m1, m2): (&Model1, &Model2) = black_box(&medium_chain).extract_bulk();
            (black_box(m1.value), black_box(m2.value))
        });
    });

    // Individual extraction (3 separate get() calls)
    group.bench_function("individual_three_get", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&medium_chain).get();
            let m2: &Model2 = black_box(&medium_chain).get();
            let m3: &Model3 = black_box(&medium_chain).get();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
            )
        });
    });

    // Individual extraction (3 separate get() calls) - no extract_three in new API yet
    group.bench_function("individual_extract_three", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&medium_chain).get();
            let m2: &Model2 = black_box(&medium_chain).get();
            let m3: &Model3 = black_box(&medium_chain).get();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
            )
        });
    });

    // Individual extraction (4 separate get() calls)
    group.bench_function("individual_four_get", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&medium_chain).get();
            let m2: &Model2 = black_box(&medium_chain).get();
            let m3: &Model3 = black_box(&medium_chain).get();
            let m4: &Model4 = black_box(&medium_chain).get();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
                black_box(m4.value),
            )
        });
    });

    // Individual extraction (4 separate get() calls) - no extract_four in new API yet
    group.bench_function("individual_extract_four", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&medium_chain).get();
            let m2: &Model2 = black_box(&medium_chain).get();
            let m3: &Model3 = black_box(&medium_chain).get();
            let m4: &Model4 = black_box(&medium_chain).get();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
                black_box(m4.value),
            )
        });
    });

    // NEW: Type-inferred bulk extraction (user's implementation)
    group.bench_function("type_inferred_extract_two", |b| {
        b.iter(|| {
            let (m1, m2): (&Model1, &Model2) = black_box(&medium_chain).extract_bulk();
            (black_box(m1.value), black_box(m2.value))
        });
    });

    // NEW: Large scale bulk extraction (8 models)
    group.bench_function("individual_eight_get", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&large_chain).get();
            let m2: &Model2 = black_box(&large_chain).get();
            let m3: &Model3 = black_box(&large_chain).get();
            let m4: &Model4 = black_box(&large_chain).get();
            let m5: &Model5 = black_box(&large_chain).get();
            let m6: &Model6 = black_box(&large_chain).get();
            let m7: &Model7 = black_box(&large_chain).get();
            let m8: &Model8 = black_box(&large_chain).get();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
                black_box(m4.value),
                black_box(m5.value),
                black_box(m6.value),
                black_box(m7.value),
                black_box(m8.value),
            )
        });
    });

    group.bench_function("bulk_extract_eight", |b| {
        b.iter(|| {
            let (m1, m2, m3, m4, m5, m6, m7, m8): (
                &Model1,
                &Model2,
                &Model3,
                &Model4,
                &Model5,
                &Model6,
                &Model7,
                &Model8,
            ) = black_box(&large_chain).extract_bulk();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
                black_box(m4.value),
                black_box(m5.value),
                black_box(m6.value),
                black_box(m7.value),
                black_box(m8.value),
            )
        });
    });

    // NEW: 16-model extraction benchmarks
    let extra_large_chain = create_extra_large_chain();

    group.bench_function("individual_sixteen_get", |b| {
        b.iter(|| {
            let m1: &Model1 = black_box(&extra_large_chain).get();
            let m2: &Model2 = black_box(&extra_large_chain).get();
            let m3: &Model3 = black_box(&extra_large_chain).get();
            let m4: &Model4 = black_box(&extra_large_chain).get();
            let m5: &Model5 = black_box(&extra_large_chain).get();
            let m6: &Model6 = black_box(&extra_large_chain).get();
            let m7: &Model7 = black_box(&extra_large_chain).get();
            let m8: &Model8 = black_box(&extra_large_chain).get();
            let m9: &Model9 = black_box(&extra_large_chain).get();
            let m10: &Model10 = black_box(&extra_large_chain).get();
            let m11: &Model11 = black_box(&extra_large_chain).get();
            let m12: &Model12 = black_box(&extra_large_chain).get();
            let m13: &Model13 = black_box(&extra_large_chain).get();
            let m14: &Model14 = black_box(&extra_large_chain).get();
            let m15: &Model15 = black_box(&extra_large_chain).get();
            let m16: &Model16 = black_box(&extra_large_chain).get();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
                black_box(m4.value),
                black_box(m5.value),
                black_box(m6.value),
                black_box(m7.value),
                black_box(m8.value),
                black_box(m9.value),
                black_box(m10.value),
                black_box(m11.value),
                black_box(m12.value),
                black_box(m13.value),
                black_box(m14.value),
                black_box(m15.value),
                black_box(m16.value),
            )
        });
    });

    group.bench_function("bulk_extract_sixteen", |b| {
        b.iter(|| {
            let (m1, m2, m3, m4, m5, m6, m7, m8, m9, m10, m11, m12, m13, m14, m15, m16): (
                &Model1,
                &Model2,
                &Model3,
                &Model4,
                &Model5,
                &Model6,
                &Model7,
                &Model8,
                &Model9,
                &Model10,
                &Model11,
                &Model12,
                &Model13,
                &Model14,
                &Model15,
                &Model16,
            ) = black_box(&extra_large_chain).extract_bulk();
            (
                black_box(m1.value),
                black_box(m2.value),
                black_box(m3.value),
                black_box(m4.value),
                black_box(m5.value),
                black_box(m6.value),
                black_box(m7.value),
                black_box(m8.value),
                black_box(m9.value),
                black_box(m10.value),
                black_box(m11.value),
                black_box(m12.value),
                black_box(m13.value),
                black_box(m14.value),
                black_box(m15.value),
                black_box(m16.value),
            )
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_single_type_access,
    bench_multi_model_extraction,
    bench_storage_creation,
    bench_cache_performance,
    bench_bulk_vs_individual
);
criterion_main!(benches);
