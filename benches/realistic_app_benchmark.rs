#![allow(dead_code, clippy::clone_on_ref_ptr, unused_variables, unused_imports, clippy::let_and_return, clippy::format_in_format_args, clippy::unnecessary_wraps, clippy::unused_self, clippy::derivable_impls, clippy::match_same_arms, clippy::cast_possible_truncation, clippy::items_after_statements, clippy::type_complexity, clippy::duplicated_attributes)]
//! Realistic Application Simulation Benchmark
//!
//! Simulates realistic Syzygy application usage patterns:
//! 1. Event processing with multi-model updates
//! 2. Command creation and batching
//! 3. Effect execution with context operations
//! 4. Typical TEA application workflows

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use syzygy::prelude::*;

// ============================================================================
// Realistic Application Models
// ============================================================================

#[derive(Debug, Clone, Default)]
struct UserModel {
    id: u32,
    name: String,
    email: String,
    is_authenticated: bool,
    login_count: u32,
}

#[derive(Debug, Clone, Default)]
struct AppConfigModel {
    theme: String,
    language: String,
    features_enabled: Vec<String>,
    last_updated: u64,
}

#[derive(Debug, Clone, Default)]
struct NotificationModel {
    unread_count: u32,
    notifications: Vec<String>,
    last_checked: u64,
}

#[derive(Debug, Clone, Default)]
struct CacheModel {
    cached_data: std::collections::HashMap<String, String>,
    cache_hits: u32,
    cache_misses: u32,
}

// ============================================================================
// Realistic Events and Effects
// ============================================================================

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum AppEvent {
    UserLogin { username: String, password: String },
    UserLogout,
    UpdateProfile { name: String, email: String },
    ChangeTheme { theme: String },
    AddNotification { message: String },
    ClearNotifications,
    CacheData { key: String, value: String },
    LoadCachedData { key: String },
    InitializeApp,
    RefreshData,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum AppEffect {
    AuthenticateUser { username: String, password: String },
    SaveProfile { user_id: u32 },
    SaveConfig,
    SendNotification { message: String },
    LogActivity { action: String },
    LoadFromDatabase { table: String },
    SaveToDatabase { table: String, data: String },
    NetworkRequest { url: String },
    UpdateUI { component: String },
}

type AppStorage = Storage<
    NotificationModel,
    Storage<CacheModel, Storage<AppConfigModel, Storage<UserModel, EmptyStorage>>>,
>;

// ============================================================================
// Realistic Update Function
// ============================================================================

fn realistic_update(
    event: AppEvent,
    ctx: &mut EventContext<AppEvent, AppEffect, AppStorage>,
) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::UserLogin { username, password } => {
            let user: &mut UserModel = ctx.model_mut();
            user.login_count += 1;

            Command::<AppEvent, AppEffect>::batch([
                Command::<AppEvent, AppEffect>::effect(AppEffect::AuthenticateUser {
                    username,
                    password,
                }),
                Command::<AppEvent, AppEffect>::effect(AppEffect::LogActivity {
                    action: "login_attempt".to_string(),
                }),
                Command::<AppEvent, AppEffect>::event(AppEvent::AddNotification {
                    message: "Login attempt recorded".to_string(),
                }),
            ])
        }

        AppEvent::UserLogout => {
            let user: &mut UserModel = ctx.model_mut();
            user.is_authenticated = false;

            let config: &mut AppConfigModel = ctx.model_mut();
            config.last_updated = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            Command::<AppEvent, AppEffect>::batch([
                Command::<AppEvent, AppEffect>::effect(AppEffect::SaveConfig),
                Command::<AppEvent, AppEffect>::effect(AppEffect::LogActivity {
                    action: "logout".to_string(),
                }),
                Command::<AppEvent, AppEffect>::event(AppEvent::ClearNotifications),
            ])
        }

        AppEvent::UpdateProfile { name, email } => {
            let user: &mut UserModel = ctx.model_mut();
            user.name = name;
            user.email = email;

            Command::<AppEvent, AppEffect>::batch([
                Command::<AppEvent, AppEffect>::effect(AppEffect::SaveProfile { user_id: user.id }),
                Command::<AppEvent, AppEffect>::effect(AppEffect::UpdateUI {
                    component: "profile".to_string(),
                }),
                Command::<AppEvent, AppEffect>::event(AppEvent::AddNotification {
                    message: "Profile updated successfully".to_string(),
                }),
            ])
        }

        AppEvent::ChangeTheme { theme } => {
            let config: &mut AppConfigModel = ctx.model_mut();
            config.theme = theme;
            config.last_updated = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            Command::<AppEvent, AppEffect>::batch([
                Command::<AppEvent, AppEffect>::effect(AppEffect::SaveConfig),
                Command::<AppEvent, AppEffect>::effect(AppEffect::UpdateUI {
                    component: "theme".to_string(),
                }),
            ])
        }

        AppEvent::AddNotification { message } => {
            let notifications: &mut NotificationModel = ctx.model_mut();
            notifications.notifications.push(message);
            notifications.unread_count += 1;

            Command::<AppEvent, AppEffect>::effect(AppEffect::UpdateUI {
                component: "notifications".to_string(),
            })
        }

        AppEvent::ClearNotifications => {
            let notifications: &mut NotificationModel = ctx.model_mut();
            notifications.notifications.clear();
            notifications.unread_count = 0;
            notifications.last_checked = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            Command::<AppEvent, AppEffect>::effect(AppEffect::UpdateUI {
                component: "notifications".to_string(),
            })
        }

        AppEvent::CacheData { key, value } => {
            let cache: &mut CacheModel = ctx.model_mut();
            cache.cached_data.insert(key, value);

            Command::<AppEvent, AppEffect>::effect(AppEffect::SaveToDatabase {
                table: "cache".to_string(),
                data: "cache_updated".to_string(),
            })
        }

        AppEvent::LoadCachedData { key } => {
            let cache: &mut CacheModel = ctx.model_mut();

            if cache.cached_data.contains_key(&key) {
                cache.cache_hits += 1;
                Command::<AppEvent, AppEffect>::none()
            } else {
                cache.cache_misses += 1;
                Command::<AppEvent, AppEffect>::effect(AppEffect::LoadFromDatabase {
                    table: "data".to_string(),
                })
            }
        }

        AppEvent::InitializeApp => {
            let user: &mut UserModel = ctx.model_mut();
            user.id = 1;

            let config: &mut AppConfigModel = ctx.model_mut();
            config.theme = "light".to_string();
            config.language = "en".to_string();
            config.features_enabled = vec!["notifications".to_string(), "caching".to_string()];

            Command::<AppEvent, AppEffect>::batch([
                Command::<AppEvent, AppEffect>::effect(AppEffect::LoadFromDatabase {
                    table: "user_preferences".to_string(),
                }),
                Command::<AppEvent, AppEffect>::effect(AppEffect::NetworkRequest {
                    url: "https://api.example.com/config".to_string(),
                }),
                Command::<AppEvent, AppEffect>::event(AppEvent::AddNotification {
                    message: "Application initialized".to_string(),
                }),
            ])
        }

        AppEvent::RefreshData => {
            // Complex operation that accesses all models
            let user: &UserModel = ctx.model();
            let config: &AppConfigModel = ctx.model();
            let notifications: &NotificationModel = ctx.model();
            let cache: &CacheModel = ctx.model();

            // Simulate data refresh logic
            let refresh_needed = user.login_count > 10
                || config.last_updated < 1_000_000
                || notifications.unread_count > 50
                || cache.cache_misses > cache.cache_hits;

            if refresh_needed {
                Command::batch([
                    Command::<AppEvent, AppEffect>::effect(AppEffect::NetworkRequest {
                        url: "https://api.example.com/refresh".to_string(),
                    }),
                    Command::<AppEvent, AppEffect>::effect(AppEffect::LoadFromDatabase {
                        table: "all_data".to_string(),
                    }),
                    Command::<AppEvent, AppEffect>::event(AppEvent::AddNotification {
                        message: "Data refresh completed".to_string(),
                    }),
                ])
            } else {
                Command::<AppEvent, AppEffect>::none()
            }
        }
    }
}

// ============================================================================
// Benchmark Functions
// ============================================================================

fn bench_realistic_event_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("realistic_event_processing");

    // Create a realistic storage setup
    let storage = EmptyStorage
        .with_model(UserModel::default())
        .with_model(AppConfigModel::default())
        .with_model(CacheModel::default())
        .with_model(NotificationModel::default());

    group.bench_function("user_login", |b| {
        b.iter(|| {
            let mut storage = storage.clone();
            let mut ctx: EventContext<AppEvent, AppEffect, AppStorage> =
                EventContext::new(&mut storage);
            let event = AppEvent::UserLogin {
                username: "testuser".to_string(),
                password: "password123".to_string(),
            };
            black_box(realistic_update(black_box(event), &mut ctx))
        });
    });

    group.bench_function("profile_update", |b| {
        b.iter(|| {
            let mut storage = storage.clone();
            let mut ctx: EventContext<AppEvent, AppEffect, AppStorage> =
                EventContext::new(&mut storage);
            let event = AppEvent::UpdateProfile {
                name: "New Name".to_string(),
                email: "new@email.com".to_string(),
            };
            black_box(realistic_update(black_box(event), &mut ctx))
        });
    });

    group.bench_function("theme_change", |b| {
        b.iter(|| {
            let mut storage = storage.clone();
            let mut ctx: EventContext<AppEvent, AppEffect, AppStorage> =
                EventContext::new(&mut storage);
            let event = AppEvent::ChangeTheme {
                theme: "dark".to_string(),
            };
            black_box(realistic_update(black_box(event), &mut ctx))
        });
    });

    group.bench_function("app_initialization", |b| {
        b.iter(|| {
            let mut storage = storage.clone();
            let mut ctx: EventContext<AppEvent, AppEffect, AppStorage> =
                EventContext::new(&mut storage);
            let event = AppEvent::InitializeApp;
            black_box(realistic_update(black_box(event), &mut ctx))
        });
    });

    group.bench_function("data_refresh", |b| {
        b.iter(|| {
            let mut storage = storage.clone();
            let mut ctx: EventContext<AppEvent, AppEffect, AppStorage> =
                EventContext::new(&mut storage);
            let event = AppEvent::RefreshData;
            black_box(realistic_update(black_box(event), &mut ctx))
        });
    });

    group.finish();
}

fn bench_command_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("command_operations");

    group.bench_function("single_effect", |b| {
        b.iter(|| {
            black_box(Command::<AppEvent, AppEffect>::effect(
                AppEffect::LogActivity {
                    action: "test".to_string(),
                },
            ))
        });
    });

    group.bench_function("single_event", |b| {
        b.iter(|| {
            black_box(Command::<AppEvent, AppEffect>::event(
                AppEvent::AddNotification {
                    message: "test".to_string(),
                },
            ))
        });
    });

    group.bench_function("batch_commands", |b| {
        b.iter(|| {
            black_box(Command::<AppEvent, AppEffect>::batch([
                Command::<AppEvent, AppEffect>::effect(AppEffect::SaveConfig),
                Command::<AppEvent, AppEffect>::effect(AppEffect::UpdateUI {
                    component: "test".to_string(),
                }),
                Command::<AppEvent, AppEffect>::event(AppEvent::AddNotification {
                    message: "done".to_string(),
                }),
            ]))
        });
    });

    group.bench_function("nested_batch_commands", |b| {
        b.iter(|| {
            black_box(Command::batch([
                Command::batch([
                    Command::<AppEvent, AppEffect>::effect(AppEffect::SaveConfig),
                    Command::<AppEvent, AppEffect>::effect(AppEffect::LogActivity {
                        action: "nested".to_string(),
                    }),
                ]),
                Command::batch([
                    Command::<AppEvent, AppEffect>::event(AppEvent::AddNotification {
                        message: "test".to_string(),
                    }),
                    Command::<AppEvent, AppEffect>::effect(AppEffect::UpdateUI {
                        component: "ui".to_string(),
                    }),
                ]),
            ]))
        });
    });

    group.finish();
}

fn bench_storage_patterns(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_patterns");

    let storage = EmptyStorage
        .with_model(UserModel::default())
        .with_model(AppConfigModel::default())
        .with_model(CacheModel::default())
        .with_model(NotificationModel::default());

    group.bench_function("single_model_access", |b| {
        b.iter(|| {
            let user: &UserModel = black_box(&storage).get();
            black_box(user.login_count)
        });
    });

    group.bench_function("multi_model_access", |b| {
        b.iter(|| {
            let user: &UserModel = black_box(&storage).get();
            let config: &AppConfigModel = black_box(&storage).get();
            let notifications: &NotificationModel = black_box(&storage).get();
            let cache: &CacheModel = black_box(&storage).get();
            black_box((
                user.login_count,
                config.last_updated,
                notifications.unread_count,
                cache.cache_hits,
            ))
        });
    });

    group.bench_function("mixed_access_pattern", |b| {
        b.iter(|| {
            let mut storage = storage.clone();
            let ctx: EventContext<AppEvent, AppEffect, AppStorage> =
                EventContext::new(&mut storage);

            // Read some models
            let user: &UserModel = ctx.model();
            let config: &AppConfigModel = ctx.model();

            // Modify some models
            let notifications: &mut NotificationModel = ctx.model_mut();
            notifications.unread_count += 1;

            let cache: &mut CacheModel = ctx.model_mut();
            cache.cache_hits += 1;

            black_box((
                user.id,
                config.theme.len(),
                notifications.unread_count,
                cache.cache_hits,
            ))
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_realistic_event_processing,
    bench_command_operations,
    bench_storage_patterns
);
criterion_main!(benches);
