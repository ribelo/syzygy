#![allow(dead_code, clippy::clone_on_ref_ptr, unused_variables, unused_imports, clippy::let_and_return, clippy::format_in_format_args, clippy::unnecessary_wraps, clippy::unused_self, clippy::derivable_impls, clippy::match_same_arms, clippy::cast_possible_truncation, clippy::items_after_statements, clippy::type_complexity, clippy::duplicated_attributes)]
//! Benchmark comparing move-by-value vs reference + clone patterns
//!
//! This benchmark demonstrates why move-by-value is more efficient for Events
//! in the Syzygy architecture, where each event is consumed by exactly one handler.

use criterion::{criterion_group, criterion_main, Criterion};
use syzygy::prelude::*;

// Test Event with owned data that would need cloning in reference approach
#[derive(Debug, Clone)]
enum TestEvent {
    UserLogin {
        username: String,
        password: String,
        session_data: Vec<String>,
    },
    DataImport {
        records: Vec<UserRecord>,
    },
}

#[derive(Debug, Clone)]
struct UserRecord {
    pub id: u64,
    pub name: String,
    pub data: String,
}

#[derive(Debug, Clone)]
enum TestEffect {
    Authenticate {
        username: String,
        password: String,
        session_data: Vec<String>,
    },
    ProcessRecords {
        records: Vec<UserRecord>,
    },
}

// Use the effects to prevent dead code warnings
fn _use_effects(effect: &TestEffect) {
    match effect {
        TestEffect::Authenticate { username, password, session_data } => {
            let _len = username.len() + password.len() + session_data.len();
        }
        TestEffect::ProcessRecords { records } => {
            for record in records {
                #[allow(clippy::cast_possible_truncation)] // Benchmark code - u64 to usize is intentional
                let _total = record.id as usize + record.name.len() + record.data.len();
            }
        }
    }
}

#[derive(Debug, Default)]
struct TestModel {
    user_count: u32,
    data_processed: bool,
}

use syzygy::storage::{EmptyStorage, Storage};

// Move-by-value approach (current, efficient)
fn move_update(
    event: TestEvent,
    ctx: &mut EventContext<TestEvent, TestEffect, Storage<TestModel, EmptyStorage>>,
) -> Command<TestEvent, TestEffect> {
    let model: &mut TestModel = ctx.model_mut();
    
    match event {
        TestEvent::UserLogin { username, password, session_data } => {
            model.user_count += 1;
            // Direct move - no clones needed
            Command::effect(TestEffect::Authenticate {
                username,
                password,
                session_data,
            })
        }
        TestEvent::DataImport { records } => {
            model.data_processed = true;
            // Direct move - no clones needed  
            Command::effect(TestEffect::ProcessRecords { records })
        }
    }
}

// Reference approach (old, less efficient due to required clones)  
fn reference_update(
    event: &TestEvent,
    ctx: &mut EventContext<TestEvent, TestEffect, Storage<TestModel, EmptyStorage>>,
) -> Command<TestEvent, TestEffect> {
    let model: &mut TestModel = ctx.model_mut();
    
    match event {
        TestEvent::UserLogin { username, password, session_data } => {
            model.user_count += 1;
            // Must clone everything to create the command
            Command::effect(TestEffect::Authenticate {
                username: username.clone(),
                password: password.clone(),
                session_data: session_data.clone(),
            })
        }
        TestEvent::DataImport { records } => {
            model.data_processed = true;
            // Must clone the entire Vec<UserRecord>
            Command::effect(TestEffect::ProcessRecords { 
                records: records.clone()
            })
        }
    }
}

fn create_large_event() -> TestEvent {
    TestEvent::DataImport {
        records: (0..1000).map(|i| UserRecord {
            id: i,
            name: format!("User {i}"),
            data: format!("Large data string for user {i} with more content to make cloning expensive"),
        }).collect(),
    }
}

fn create_login_event() -> TestEvent {
    TestEvent::UserLogin {
        username: "user@example.com".to_string(),
        password: "secure_password_123".to_string(),
        session_data: vec![
            "session_token_abc123".to_string(),
            "csrf_token_def456".to_string(),
            "refresh_token_ghi789".to_string(),
        ],
    }
}

fn bench_move_approach(c: &mut Criterion) {
    c.bench_function("move_login_event", |b| {
        b.iter(|| {
            let mut storage = EmptyStorage.with_model(TestModel::default());
            let mut ctx = EventContext::new(&mut storage);
            let event = create_login_event();
            let command = move_update(event, &mut ctx);
            let effects = command.into_effects();
            if let Some(effect) = effects.first() {
                _use_effects(effect);
            }
        })
    });
    
    c.bench_function("move_large_event", |b| {
        b.iter(|| {
            let mut storage = EmptyStorage.with_model(TestModel::default());
            let mut ctx = EventContext::new(&mut storage);
            let event = create_large_event();
            let command = move_update(event, &mut ctx);
            let effects = command.into_effects();
            if let Some(effect) = effects.first() {
                _use_effects(effect);
            }
        })
    });
}

fn bench_reference_approach(c: &mut Criterion) {
    c.bench_function("reference_login_event", |b| {
        b.iter(|| {
            let mut storage = EmptyStorage.with_model(TestModel::default());
            let mut ctx = EventContext::new(&mut storage);
            let event = create_login_event();
            let command = reference_update(&event, &mut ctx);
            let effects = command.into_effects();
            if let Some(effect) = effects.first() {
                _use_effects(effect);
            }
        })
    });
    
    c.bench_function("reference_large_event", |b| {
        b.iter(|| {
            let mut storage = EmptyStorage.with_model(TestModel::default());
            let mut ctx = EventContext::new(&mut storage);
            let event = create_large_event();
            let command = reference_update(&event, &mut ctx);
            let effects = command.into_effects();
            if let Some(effect) = effects.first() {
                _use_effects(effect);
            }
        })
    });
}

criterion_group!(benches, bench_move_approach, bench_reference_approach);
criterion_main!(benches);