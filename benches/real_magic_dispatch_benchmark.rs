#![allow(dead_code, clippy::clone_on_ref_ptr, unused_variables, unused_imports, clippy::let_and_return, clippy::format_in_format_args, clippy::unnecessary_wraps, clippy::unused_self, clippy::derivable_impls, clippy::match_same_arms, clippy::cast_possible_truncation, clippy::items_after_statements, clippy::type_complexity, clippy::duplicated_attributes)]
use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::sync::Arc;
use syzygy::prelude::*;
use tokio::sync::Mutex;

// Test resources
#[derive(Debug, Clone)]
struct HttpClient {
    base_url: String,
}

#[derive(Debug, Clone)]
struct DatabaseConnection {
    connection_string: String,
}

#[derive(Debug, Clone)]
struct Logger {
    level: String,
}

// Test effects
#[derive(Debug, Clone)]
enum TestEffect {
    FetchUserData { user_id: u64 },
    SaveToDatabase { data: String },
    LogMessage { message: String },
    ComplexOperation { id: u64, data: String },
}

type TestResources = Storage<
    Arc<Mutex<HttpClient>>,
    Storage<Arc<Mutex<DatabaseConnection>>, Storage<Arc<Mutex<Logger>>, EmptyStorage>>,
>;

fn create_test_resources() -> TestResources {
    EmptyStorage
        .with_model(Arc::new(Mutex::new(Logger {
            level: "INFO".to_string(),
        })))
        .with_model(Arc::new(Mutex::new(DatabaseConnection {
            connection_string: "postgresql://localhost/test".to_string(),
        })))
        .with_model(Arc::new(Mutex::new(HttpClient {
            base_url: "https://api.example.com".to_string(),
        })))
}

// === REAL MAGIC HANDLERS (using actual magic system) ===

// These would use the actual FromEffectContext trait implementations
// For now, simulating what the real magic system would generate
async fn magic_fetch_handler(
    _data: TestEffect, // In real magic system this would be FetchUserData variant
    http: Arc<Mutex<HttpClient>>,
) {
    let client = http.lock().await;
    black_box(&*client);
    // Simulate some async work
    tokio::task::yield_now().await;
}

async fn magic_save_handler(_data: TestEffect, db: Arc<Mutex<DatabaseConnection>>) {
    let database = db.lock().await;
    black_box(&*database);
    tokio::task::yield_now().await;
}

async fn magic_log_handler(_data: TestEffect, logger: Arc<Mutex<Logger>>) {
    let log = logger.lock().await;
    black_box(&*log);
    tokio::task::yield_now().await;
}

async fn magic_complex_handler(
    _data: TestEffect,
    http: Arc<Mutex<HttpClient>>,
    db: Arc<Mutex<DatabaseConnection>>,
    logger: Arc<Mutex<Logger>>,
) {
    let client = http.lock().await;
    let database = db.lock().await;
    let log = logger.lock().await;
    black_box((&*client, &*database, &*log));
    tokio::task::yield_now().await;
}

// === MANUAL HANDLERS (explicit resource extraction) ===

async fn manual_fetch_handler(_data: TestEffect, http: &Arc<Mutex<HttpClient>>) {
    let client = http.lock().await;
    black_box(&*client);
    tokio::task::yield_now().await;
}

async fn manual_save_handler(_data: TestEffect, db: &Arc<Mutex<DatabaseConnection>>) {
    let database = db.lock().await;
    black_box(&*database);
    tokio::task::yield_now().await;
}

async fn manual_log_handler(_data: TestEffect, logger: &Arc<Mutex<Logger>>) {
    let log = logger.lock().await;
    black_box(&*log);
    tokio::task::yield_now().await;
}

async fn manual_complex_handler(
    _data: TestEffect,
    http: &Arc<Mutex<HttpClient>>,
    db: &Arc<Mutex<DatabaseConnection>>,
    logger: &Arc<Mutex<Logger>>,
) {
    let client = http.lock().await;
    let database = db.lock().await;
    let log = logger.lock().await;
    black_box((&*client, &*database, &*log));
    tokio::task::yield_now().await;
}

// === DISPATCH SYSTEMS ===

// Magic dispatch system (simulates what effect_magic_handler! macro would generate)
async fn magic_dispatch(effect: TestEffect, ctx: EffectContext<(), TestResources>) {
    match effect {
        TestEffect::FetchUserData { .. } => {
            let http: &Arc<Mutex<HttpClient>> = ctx.resource();
            magic_fetch_handler(effect, http.clone()).await;
        }
        TestEffect::SaveToDatabase { .. } => {
            let db: &Arc<Mutex<DatabaseConnection>> = ctx.resource();
            magic_save_handler(effect, db.clone()).await;
        }
        TestEffect::LogMessage { .. } => {
            let logger: &Arc<Mutex<Logger>> = ctx.resource();
            magic_log_handler(effect, logger.clone()).await;
        }
        TestEffect::ComplexOperation { .. } => {
            let http: &Arc<Mutex<HttpClient>> = ctx.resource();
            let db: &Arc<Mutex<DatabaseConnection>> = ctx.resource();
            let logger: &Arc<Mutex<Logger>> = ctx.resource();
            magic_complex_handler(effect, http.clone(), db.clone(), logger.clone()).await;
        }
    }
}

// Manual dispatch system
async fn manual_dispatch(effect: TestEffect, ctx: &EffectContext<(), TestResources>) {
    match effect {
        TestEffect::FetchUserData { .. } => {
            let http: &Arc<Mutex<HttpClient>> = ctx.resource();
            manual_fetch_handler(effect, http).await;
        }
        TestEffect::SaveToDatabase { .. } => {
            let db: &Arc<Mutex<DatabaseConnection>> = ctx.resource();
            manual_save_handler(effect, db).await;
        }
        TestEffect::LogMessage { .. } => {
            let logger: &Arc<Mutex<Logger>> = ctx.resource();
            manual_log_handler(effect, logger).await;
        }
        TestEffect::ComplexOperation { .. } => {
            let http: &Arc<Mutex<HttpClient>> = ctx.resource();
            let db: &Arc<Mutex<DatabaseConnection>> = ctx.resource();
            let logger: &Arc<Mutex<Logger>> = ctx.resource();
            manual_complex_handler(effect, http, db, logger).await;
        }
    }
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn bench_dispatch_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("dispatch_overhead");

    let effects = vec![
        TestEffect::FetchUserData { user_id: 1 },
        TestEffect::SaveToDatabase {
            data: "test".to_string(),
        },
        TestEffect::LogMessage {
            message: "test".to_string(),
        },
        TestEffect::ComplexOperation {
            id: 1,
            data: "test".to_string(),
        },
    ];

    // Magic dispatch
    group.bench_function("magic_dispatch", |b| {
        let rt = rt();
        b.iter(|| {
            rt.block_on(async {
                let resources = create_test_resources();
                let ctx = EffectContext::<(), TestResources>::new(None, resources);

                for effect in &effects {
                    magic_dispatch(effect.clone(), ctx.clone()).await;
                }
            });
        });
    });

    // Manual dispatch
    group.bench_function("manual_dispatch", |b| {
        let rt = rt();
        b.iter(|| {
            rt.block_on(async {
                let resources = create_test_resources();
                let ctx = EffectContext::<(), TestResources>::new(None, resources);

                for effect in &effects {
                    manual_dispatch(effect.clone(), &ctx).await;
                }
            });
        });
    });

    group.finish();
}

fn bench_single_effect_dispatch(c: &mut Criterion) {
    let mut group = c.benchmark_group("single_effect");

    // Simple effect
    group.bench_function("magic_simple", |b| {
        let rt = rt();
        b.iter(|| {
            rt.block_on(async {
                let resources = create_test_resources();
                let ctx = EffectContext::<(), TestResources>::new(None, resources);
                let effect = TestEffect::FetchUserData { user_id: 1 };

                magic_dispatch(effect, ctx).await;
            });
        });
    });

    group.bench_function("manual_simple", |b| {
        let rt = rt();
        b.iter(|| {
            rt.block_on(async {
                let resources = create_test_resources();
                let ctx = EffectContext::<(), TestResources>::new(None, resources);
                let effect = TestEffect::FetchUserData { user_id: 1 };

                manual_dispatch(effect, &ctx).await;
            });
        });
    });

    // Complex effect
    group.bench_function("magic_complex", |b| {
        let rt = rt();
        b.iter(|| {
            rt.block_on(async {
                let resources = create_test_resources();
                let ctx = EffectContext::<(), TestResources>::new(None, resources);
                let effect = TestEffect::ComplexOperation {
                    id: 1,
                    data: "test".to_string(),
                };

                magic_dispatch(effect, ctx).await;
            });
        });
    });

    group.bench_function("manual_complex", |b| {
        let rt = rt();
        b.iter(|| {
            rt.block_on(async {
                let resources = create_test_resources();
                let ctx = EffectContext::<(), TestResources>::new(None, resources);
                let effect = TestEffect::ComplexOperation {
                    id: 1,
                    data: "test".to_string(),
                };

                manual_dispatch(effect, &ctx).await;
            });
        });
    });

    group.finish();
}

fn bench_resource_extraction_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("resource_extraction");

    // Test different resource extraction patterns
    group.bench_function("single_resource_magic", |b| {
        let rt = rt();
        b.iter(|| {
            rt.block_on(async {
                let resources = create_test_resources();
                let ctx = EffectContext::<(), TestResources>::new(None, resources);
                let effect = TestEffect::FetchUserData { user_id: 1 };

                // Simulate magic extraction (clone cost)
                let http: &Arc<Mutex<HttpClient>> = ctx.resource();
                magic_fetch_handler(effect, http.clone()).await;
            });
        });
    });

    group.bench_function("single_resource_manual", |b| {
        let rt = rt();
        b.iter(|| {
            rt.block_on(async {
                let resources = create_test_resources();
                let ctx = EffectContext::<(), TestResources>::new(None, resources);
                let effect = TestEffect::FetchUserData { user_id: 1 };

                // Manual extraction (reference cost)
                let http: &Arc<Mutex<HttpClient>> = ctx.resource();
                manual_fetch_handler(effect, http).await;
            });
        });
    });

    group.finish();
}

criterion_group!(
    dispatch_benches,
    bench_dispatch_overhead,
    bench_single_effect_dispatch,
    bench_resource_extraction_patterns,
);
criterion_main!(dispatch_benches);
