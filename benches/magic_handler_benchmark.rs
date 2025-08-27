use criterion::{Criterion, black_box, criterion_group, criterion_main};
use syzygy::prelude::*;

// Test models for benchmarking
#[derive(Debug, Clone, Default)]
struct UserModel {
    id: u64,
    name: String,
    email: String,
}

#[derive(Debug, Clone, Default)]
struct ConfigModel {
    theme: String,
    version: u32,
    debug_mode: bool,
}

#[derive(Debug, Clone, Default)]
struct CacheModel {
    entries: u32,
    size_mb: f64,
}

#[derive(Debug, Clone, Default)]
struct MetricsModel {
    request_count: u64,
    error_count: u32,
}

#[derive(Debug, Clone, Default)]
struct SessionModel {
    token: String,
    expires_at: u64,
}

// Test events and effects
#[derive(Debug, Clone)]
enum TestEvent {
    UpdateUser { id: u64, name: String },
    ChangeTheme { theme: String },
    CacheHit { key: String },
    RecordMetric { value: u64 },
    LoginUser { username: String },
}

#[derive(Debug, Clone)]
enum TestEffect {
    SaveUser,
    UpdateConfig,
    ClearCache,
    SendMetrics,
    RefreshSession,
}

// Helper function to create test storage
type TestStorage = Storage<
    UserModel,
    Storage<
        ConfigModel,
        Storage<CacheModel, Storage<MetricsModel, Storage<SessionModel, EmptyStorage>>>,
    >,
>;

fn create_test_storage() -> TestStorage {
    EmptyStorage
        .with_model(SessionModel {
            token: "abc123".to_string(),
            expires_at: 1234567890,
        })
        .with_model(MetricsModel {
            request_count: 1000,
            error_count: 5,
        })
        .with_model(CacheModel {
            entries: 100,
            size_mb: 10.5,
        })
        .with_model(ConfigModel {
            theme: "dark".to_string(),
            version: 1,
            debug_mode: false,
        })
        .with_model(UserModel {
            id: 1,
            name: "Test User".to_string(),
            email: "test@example.com".to_string(),
        })
}

// Helper macro to create a magic handler that extracts models
macro_rules! make_handler {
    ($name:ident, $($param_name:ident: $param_type:ty),*) => {
        fn $name($($param_name: $param_type),*) -> Command<TestEvent, TestEffect> {
            $(
                black_box($param_name);
            )*
            Command::none()
        }
    };
}

// Create magic handlers with different parameter counts
make_handler!(magic_handler_one, user: &UserModel);
make_handler!(magic_handler_two, user: &UserModel, config: &ConfigModel);
make_handler!(magic_handler_three, user: &UserModel, config: &ConfigModel, cache: &CacheModel);
make_handler!(magic_handler_mixed, user: &mut UserModel, config: &ConfigModel);
make_handler!(magic_handler_complex,
    user: &mut UserModel,
    config: &ConfigModel,
    cache: &CacheModel,
    metrics: &MetricsModel,
    session: &SessionModel
);

// Create manual handlers that do equivalent work
fn manual_handler_one(user: &UserModel) -> Command<TestEvent, TestEffect> {
    black_box(user);
    Command::none()
}

fn manual_handler_two(user: &UserModel, config: &ConfigModel) -> Command<TestEvent, TestEffect> {
    black_box((user, config));
    Command::none()
}

fn manual_handler_three(
    user: &UserModel,
    config: &ConfigModel,
    cache: &CacheModel,
) -> Command<TestEvent, TestEffect> {
    black_box((user, config, cache));
    Command::none()
}

fn manual_handler_mixed(
    user: &mut UserModel,
    config: &ConfigModel,
) -> Command<TestEvent, TestEffect> {
    black_box((user, config));
    Command::none()
}

fn manual_handler_complex(
    user: &mut UserModel,
    config: &ConfigModel,
    cache: &CacheModel,
    metrics: &MetricsModel,
    session: &SessionModel,
) -> Command<TestEvent, TestEffect> {
    black_box((user, config, cache, metrics, session));
    Command::none()
}

// Import magic handler system (not used in this benchmark)

fn bench_one_param(c: &mut Criterion) {
    let mut group = c.benchmark_group("one_parameter");

    // Magic handler benchmark using update-style wrapper
    group.bench_function("magic_handler", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();
            let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

            // Wrap the magic handler to match update signature
            fn update_wrapper(
                _event: TestEvent,
                ctx: &mut EventContext<TestEvent, TestEffect, TestStorage>,
            ) -> Command<TestEvent, TestEffect> {
                let user: &UserModel = ctx.model();
                magic_handler_one(user)
            }

            let event = TestEvent::UpdateUser {
                id: 1,
                name: "test".to_string(),
            };
            let result = update_wrapper(event, &mut EventContext::new(&mut storage));
            black_box(result)
        });
    });

    // Manual extraction benchmark
    group.bench_function("manual_extraction", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();
            let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);
            let user: &UserModel = ctx.model();
            let result = manual_handler_one(user);
            black_box(result)
        });
    });

    group.finish();
}

fn bench_two_params(c: &mut Criterion) {
    let mut group = c.benchmark_group("two_parameters");

    // Magic handler benchmark
    group.bench_function("magic_handler", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();

            // Wrap the magic handler to match update signature
            fn update_wrapper(
                _event: TestEvent,
                ctx: &mut EventContext<TestEvent, TestEffect, TestStorage>,
            ) -> Command<TestEvent, TestEffect> {
                let user: &UserModel = ctx.model();
                let config: &ConfigModel = ctx.model();
                magic_handler_two(user, config)
            }

            let event = TestEvent::UpdateUser {
                id: 1,
                name: "test".to_string(),
            };
            let result = update_wrapper(event, &mut EventContext::new(&mut storage));
            black_box(result)
        });
    });

    // Manual extraction benchmark
    group.bench_function("manual_extraction", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();
            let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);
            let user: &UserModel = ctx.model();
            let config: &ConfigModel = ctx.model();
            let result = manual_handler_two(user, config);
            black_box(result)
        });
    });

    group.finish();
}

fn bench_three_params(c: &mut Criterion) {
    let mut group = c.benchmark_group("three_parameters");

    // Magic handler benchmark
    group.bench_function("magic_handler", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();

            // Wrap the magic handler to match update signature
            fn update_wrapper(
                _event: TestEvent,
                ctx: &mut EventContext<TestEvent, TestEffect, TestStorage>,
            ) -> Command<TestEvent, TestEffect> {
                let user: &UserModel = ctx.model();
                let config: &ConfigModel = ctx.model();
                let cache: &CacheModel = ctx.model();
                magic_handler_three(user, config, cache)
            }

            let event = TestEvent::UpdateUser {
                id: 1,
                name: "test".to_string(),
            };
            let result = update_wrapper(event, &mut EventContext::new(&mut storage));
            black_box(result)
        });
    });

    // Manual extraction benchmark
    group.bench_function("manual_extraction", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();
            let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);
            let user: &UserModel = ctx.model();
            let config: &ConfigModel = ctx.model();
            let cache: &CacheModel = ctx.model();
            let result = manual_handler_three(user, config, cache);
            black_box(result)
        });
    });

    group.finish();
}

fn bench_mixed_refs(c: &mut Criterion) {
    let mut group = c.benchmark_group("mixed_references");

    // Magic handler benchmark
    group.bench_function("magic_handler", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();

            // Wrap the magic handler to match update signature
            fn update_wrapper(
                _event: TestEvent,
                ctx: &mut EventContext<TestEvent, TestEffect, TestStorage>,
            ) -> Command<TestEvent, TestEffect> {
                let user: &mut UserModel = ctx.model_mut();
                let config: &ConfigModel = ctx.model();
                magic_handler_mixed(user, config)
            }

            let event = TestEvent::UpdateUser {
                id: 1,
                name: "test".to_string(),
            };
            let result = update_wrapper(event, &mut EventContext::new(&mut storage));
            black_box(result)
        });
    });

    // Manual extraction benchmark
    group.bench_function("manual_extraction", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();
            let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);
            let user: &mut UserModel = ctx.model_mut();
            let config: &ConfigModel = ctx.model();
            let result = manual_handler_mixed(user, config);
            black_box(result)
        });
    });

    group.finish();
}

fn bench_complex_params(c: &mut Criterion) {
    let mut group = c.benchmark_group("five_parameters");

    // Magic handler benchmark
    group.bench_function("magic_handler", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();

            // Wrap the magic handler to match update signature
            fn update_wrapper(
                _event: TestEvent,
                ctx: &mut EventContext<TestEvent, TestEffect, TestStorage>,
            ) -> Command<TestEvent, TestEffect> {
                let user: &mut UserModel = ctx.model_mut();
                let config: &ConfigModel = ctx.model();
                let cache: &CacheModel = ctx.model();
                let metrics: &MetricsModel = ctx.model();
                let session: &SessionModel = ctx.model();
                magic_handler_complex(user, config, cache, metrics, session)
            }

            let event = TestEvent::UpdateUser {
                id: 1,
                name: "test".to_string(),
            };
            let result = update_wrapper(event, &mut EventContext::new(&mut storage));
            black_box(result)
        });
    });

    // Manual extraction benchmark
    group.bench_function("manual_extraction", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();
            let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);
            let user: &mut UserModel = ctx.model_mut();
            let config: &ConfigModel = ctx.model();
            let cache: &CacheModel = ctx.model();
            let metrics: &MetricsModel = ctx.model();
            let session: &SessionModel = ctx.model();
            let result = manual_handler_complex(user, config, cache, metrics, session);
            black_box(result)
        });
    });

    group.finish();
}

fn bench_extraction_overhead_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("extraction_overhead_only");

    // Magic handler benchmark - just the extraction without any work
    group.bench_function("magic_extraction_5_params", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();

            // Wrap the magic handler to match update signature - minimal work
            fn update_wrapper(
                _event: TestEvent,
                ctx: &mut EventContext<TestEvent, TestEffect, TestStorage>,
            ) -> Command<TestEvent, TestEffect> {
                let user: &mut UserModel = ctx.model_mut();
                let config: &ConfigModel = ctx.model();
                let cache: &CacheModel = ctx.model();
                let metrics: &MetricsModel = ctx.model();
                let session: &SessionModel = ctx.model();

                // Just black_box the extracted values to prevent optimization
                black_box((user, config, cache, metrics, session));
                Command::none()
            }

            let event = TestEvent::UpdateUser {
                id: 1,
                name: "test".to_string(),
            };
            let result = update_wrapper(event, &mut EventContext::new(&mut storage));
            black_box(result)
        });
    });

    // Manual extraction overhead
    group.bench_function("manual_extraction_5_params", |b| {
        b.iter(|| {
            let mut storage = create_test_storage();
            let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

            // Manual extraction of the same 5 parameters
            let user: &mut UserModel = ctx.model_mut();
            let config: &ConfigModel = ctx.model();
            let cache: &CacheModel = ctx.model();
            let metrics: &MetricsModel = ctx.model();
            let session: &SessionModel = ctx.model();

            // Same minimal work
            black_box((user, config, cache, metrics, session));
        });
    });

    group.finish();
}

criterion_group!(
    magic_handler_benches,
    bench_one_param,
    bench_two_params,
    bench_three_params,
    bench_mixed_refs,
    bench_complex_params,
    bench_extraction_overhead_only,
);
criterion_main!(magic_handler_benches);
