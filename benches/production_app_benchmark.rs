//! Production Application Benchmark
//!
//! Simulates a complete task management application to measure
//! Syzygy's real-world performance characteristics.

use criterion::{Criterion, criterion_group, criterion_main};
use syzygy::prelude::*;
use std::sync::{Arc, atomic::AtomicU64};
use tokio::sync::Mutex;

// --- Helper Enums and Structs ---

#[derive(Debug, Clone, Default, PartialEq)]
#[allow(dead_code)]
enum TaskFilter {
    #[default]
    All,
    Active,
    Completed,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Notification {
    message: String,
    timestamp: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
#[allow(dead_code)]
enum SyncStatus {
    #[default]
    Idle,
    Syncing,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum Priority {
    Low,
    Medium,
    High,
}

impl Default for Priority {
    fn default() -> Self {
        Priority::Medium
    }
}

// --- Model Definitions ---

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct TaskModel {
    tasks: Vec<Task>,
    active_task_id: Option<u32>,
    filter: TaskFilter,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct UserModel {
    user_id: u32,
    username: String,
    session_token: Option<String>,
    last_activity: u64,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
struct AppStateModel {
    is_loading: bool,
    error_message: Option<String>,
    notifications: Vec<Notification>,
    sync_status: SyncStatus,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct Task {
    id: u32,
    title: String,
    completed: bool,
    created_at: u64,
    priority: Priority,
}

impl Default for Task {
    fn default() -> Self {
        Self {
            id: 0,
            title: String::new(),
            completed: false,
            created_at: 0,
            priority: Priority::default(),
        }
    }
}


// --- Resource Definitions ---

#[derive(Clone)]
#[allow(dead_code)]
struct Database {
    connection_pool: Arc<Mutex<Vec<String>>>, // Simulate connection pool
    query_count: Arc<AtomicU64>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct ApiClient {
    base_url: String,
    request_count: Arc<AtomicU64>,
}

#[derive(Clone)]
#[allow(dead_code)]
struct NotificationService {
    pending_notifications: Arc<Mutex<Vec<String>>>,
}


// --- Events and Effects ---

#[derive(Debug, Clone)]
enum AppEvent {
    Login { username: String, password: String },
    LoginSuccess { user: UserModel },
    LoginFailure { error: String },
    LoadTasks,
    TasksLoaded { tasks: Vec<Task> },
    Sync,
    SyncSuccess,
    SyncFailure { error: String },
    SyncWithConflict,
    ConflictResolved,
    AddTask(String),
    CompleteTask(u32),
    DeleteTask(u32),
    ImportTasks(Vec<Task>),
    MarkTasksComplete(Vec<u32>),
}

#[derive(Debug, Clone)]
enum AppEffect {
    Authenticate { username: String, password: String },
    FetchTasks,
    SyncWithServer { with_conflict: bool },
    ResolveConflict,
    SaveTask(Task),
    DeleteTask(u32),
    BulkSaveTasks(Vec<Task>),
    BulkUpdateTasks(Vec<u32>),
}

// --- Update Function ---

type AppStorage = Storage<AppStateModel, Storage<TaskModel, Storage<UserModel, EmptyStorage>>>;

fn update(event: &AppEvent, ctx: &mut EventContext<AppEvent, AppEffect, AppStorage>) -> Command<AppEvent, AppEffect> {
    let user_model: &mut UserModel = ctx.model_mut();
    let task_model: &mut TaskModel = ctx.model_mut();
    let app_state: &mut AppStateModel = ctx.model_mut();

    match event {
        AppEvent::Login { username, password } => {
            app_state.is_loading = true;
            Command::effect(AppEffect::Authenticate { 
                username: username.clone(), 
                password: password.clone() 
            })
        }
        AppEvent::LoginSuccess { user } => {
            *user_model = user.clone();
            app_state.is_loading = false;
            app_state.error_message = None;
            Command::event(AppEvent::LoadTasks)
        }
        AppEvent::LoginFailure { error } => {
            app_state.is_loading = false;
            app_state.error_message = Some(error.clone());
            Command::none()
        }
        AppEvent::LoadTasks => {
            app_state.is_loading = true;
            Command::effect(AppEffect::FetchTasks)
        }
        AppEvent::TasksLoaded { tasks } => {
            task_model.tasks = tasks.clone();
            app_state.is_loading = false;
            Command::event(AppEvent::Sync)
        }
        AppEvent::Sync => {
            app_state.sync_status = SyncStatus::Syncing;
            Command::effect(AppEffect::SyncWithServer { with_conflict: false })
        }
        AppEvent::SyncSuccess => {
            app_state.sync_status = SyncStatus::Idle;
            Command::none()
        }
        AppEvent::SyncFailure { error } => {
            app_state.sync_status = SyncStatus::Error;
            app_state.error_message = Some(error.clone());
            if *error == "Conflict" {
                Command::effect(AppEffect::ResolveConflict)
            } else {
                Command::none()
            }
        }
        AppEvent::SyncWithConflict => {
            app_state.sync_status = SyncStatus::Syncing;
            Command::effect(AppEffect::SyncWithServer { with_conflict: true })
        }
        AppEvent::ConflictResolved => {
            app_state.sync_status = SyncStatus::Idle;
            Command::none()
        }
        AppEvent::AddTask(title) => {
            let new_task = Task {
                id: task_model.tasks.len() as u32 + 1,
                title: title.clone(),
                completed: false,
                created_at: 0, // Not used in this benchmark
                priority: Priority::Medium,
            };
            task_model.tasks.push(new_task.clone());
            Command::effect(AppEffect::SaveTask(new_task))
        }
        AppEvent::CompleteTask(id) => {
            if let Some(task) = task_model.tasks.iter_mut().find(|t| t.id == *id) {
                task.completed = true;
                Command::effect(AppEffect::SaveTask(task.clone()))
            } else {
                Command::none()
            }
        }
        AppEvent::DeleteTask(id) => {
            task_model.tasks.retain(|t| t.id != *id);
            Command::effect(AppEffect::DeleteTask(*id))
        }
        AppEvent::ImportTasks(tasks) => {
            task_model.tasks = tasks.clone();
            Command::effect(AppEffect::BulkSaveTasks(tasks.clone()))
        }
        AppEvent::MarkTasksComplete(ids) => {
            for id in ids {
                if let Some(task) = task_model.tasks.iter_mut().find(|t| t.id == *id) {
                    task.completed = true;
                }
            }
            Command::effect(AppEffect::BulkUpdateTasks(ids.clone()))
        }
    }
}


// --- Effect Handler ---

type ResourceStorage = Storage<NotificationService, Storage<ApiClient, Storage<Database, EmptyStorage>>>;

async fn handle_effects(effect: AppEffect, ctx: EffectContext<AppEvent, ResourceStorage>) {
    match effect {
        AppEffect::Authenticate { username, password } => {
            // Simulate API call
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;

            if password == "password" {
                let user = UserModel {
                    user_id: 1,
                    username,
                    session_token: Some("token".to_string()),
                    last_activity: 0,
                };
                let _ = ctx.send_event(AppEvent::LoginSuccess { user });
            } else {
                let _ = ctx.send_event(AppEvent::LoginFailure { error: "Invalid password".to_string() });
            }
        }
        AppEffect::FetchTasks => {
            // Simulate DB query
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let tasks = vec![
                Task { id: 1, title: "Buy milk".to_string(), ..Default::default() },
                Task { id: 2, title: "Walk the dog".to_string(), ..Default::default() },
            ];
            let _ = ctx.send_event(AppEvent::TasksLoaded { tasks });
        }
        AppEffect::SyncWithServer { with_conflict } => {
            // Simulate network request
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if with_conflict {
                let _ = ctx.send_event(AppEvent::SyncFailure { error: "Conflict".to_string() });
            } else {
                let _ = ctx.send_event(AppEvent::SyncSuccess);
            }
        }
        AppEffect::ResolveConflict => {
            // Simulate some complex conflict resolution logic
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let _ = ctx.send_event(AppEvent::ConflictResolved);
        }
        AppEffect::SaveTask(_task) => {
            // Simulate DB query
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        AppEffect::DeleteTask(_id) => {
            // Simulate DB query
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        AppEffect::BulkSaveTasks(_tasks) => {
            // Simulate DB query
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
        AppEffect::BulkUpdateTasks(_ids) => {
            // Simulate DB query
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }
}


// --- Benchmark Functions (stubs) ---

fn bench_user_login_flow(c: &mut Criterion) {
    c.bench_function("user_login_flow", |b| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        b.to_async(rt).iter_batched(
            || {
                // Setup: Create the Syzygy system for each iteration
                let (core, shell) = Syzygy::builder()
                    .model(UserModel::default())
                    .model(TaskModel::default())
                    .model(AppStateModel::default())
                    .resource(Database {
                        connection_pool: Arc::new(Mutex::new(Vec::new())),
                        query_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(ApiClient {
                        base_url: "http://localhost".to_string(),
                        request_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(NotificationService {
                        pending_notifications: Arc::new(Mutex::new(Vec::new())),
                    })
                    .update(update)
                    .build();

                let shell = shell.with_effect_handler(handle_effects);
                let runner = Runner::new(core, shell);
                runner
            },
            |mut runner| async move {
                // Action: Run the login flow
                runner.core().send_event(AppEvent::Login {
                    username: "testuser".to_string(),
                    password: "password".to_string(),
                }).unwrap();

                // Tick the runner until the flow is likely complete
                for _ in 0..5 {
                    if !runner.tick(syzygy::spawn::spawner()).await.unwrap() {
                        break;
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_task_crud_operations(c: &mut Criterion) {
    c.bench_function("task_crud_operations", |b| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        b.to_async(rt).iter_batched(
            || {
                // Setup: Create the Syzygy system with some initial tasks
                let (core, shell) = Syzygy::builder()
                    .model(UserModel::default())
                    .model(TaskModel {
                        tasks: vec![
                            Task { id: 1, title: "Initial task".to_string(), ..Default::default() }
                        ],
                        ..Default::default()
                    })
                    .model(AppStateModel::default())
                    .resource(Database {
                        connection_pool: Arc::new(Mutex::new(Vec::new())),
                        query_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(ApiClient {
                        base_url: "http://localhost".to_string(),
                        request_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(NotificationService {
                        pending_notifications: Arc::new(Mutex::new(Vec::new())),
                    })
                    .update(update)
                    .build();

                let shell = shell.with_effect_handler(handle_effects);
                let runner = Runner::new(core, shell);
                runner
            },
            |mut runner| async move {
                // Action: Run a sequence of CRUD operations
                runner.core().send_event(AppEvent::AddTask("New task".to_string())).unwrap();
                runner.core().send_event(AppEvent::CompleteTask(1)).unwrap();
                runner.core().send_event(AppEvent::DeleteTask(2)).unwrap();

                // Tick the runner to process the events
                for _ in 0..5 {
                    if !runner.tick(syzygy::spawn::spawner()).await.unwrap() {
                        break;
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_bulk_task_operations(c: &mut Criterion) {
    c.bench_function("bulk_task_operations", |b| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let tasks_to_import: Vec<Task> = (0..100)
            .map(|i| Task {
                id: i,
                title: format!("Task {i}"),
                ..Default::default()
            })
            .collect();

        let ids_to_complete: Vec<u32> = (0..50).collect();

        b.to_async(rt).iter_batched(
            || {
                // Setup: Create the Syzygy system
                let (core, shell) = Syzygy::builder()
                    .model(UserModel::default())
                    .model(TaskModel::default())
                    .model(AppStateModel::default())
                    .resource(Database {
                        connection_pool: Arc::new(Mutex::new(Vec::new())),
                        query_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(ApiClient {
                        base_url: "http://localhost".to_string(),
                        request_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(NotificationService {
                        pending_notifications: Arc::new(Mutex::new(Vec::new())),
                    })
                    .update(update)
                    .build();

                let shell = shell.with_effect_handler(handle_effects);
                let runner = Runner::new(core, shell);
                (runner, tasks_to_import.clone(), ids_to_complete.clone())
            },
            |(mut runner, tasks, ids)| async move {
                // Action: Run bulk operations
                runner.core().send_event(AppEvent::ImportTasks(tasks)).unwrap();
                runner.core().send_event(AppEvent::MarkTasksComplete(ids)).unwrap();

                // Tick the runner to process the events
                for _ in 0..5 {
                    if !runner.tick(syzygy::spawn::spawner()).await.unwrap() {
                        break;
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_background_sync(c: &mut Criterion) {
    c.bench_function("background_sync_with_conflict", |b| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        b.to_async(rt).iter_batched(
            || {
                // Setup: Create the Syzygy system
                let (core, shell) = Syzygy::builder()
                    .model(UserModel::default())
                    .model(TaskModel::default())
                    .model(AppStateModel::default())
                    .resource(Database {
                        connection_pool: Arc::new(Mutex::new(Vec::new())),
                        query_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(ApiClient {
                        base_url: "http://localhost".to_string(),
                        request_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(NotificationService {
                        pending_notifications: Arc::new(Mutex::new(Vec::new())),
                    })
                    .update(update)
                    .build();

                let shell = shell.with_effect_handler(handle_effects);
                let runner = Runner::new(core, shell);
                runner
            },
            |mut runner| async move {
                // Action: Run sync with conflict
                runner.core().send_event(AppEvent::SyncWithConflict).unwrap();

                // Tick the runner to process the events
                for _ in 0..5 {
                    if !runner.tick(syzygy::spawn::spawner()).await.unwrap() {
                        break;
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_mixed_realistic_workload(c: &mut Criterion) {
    c.bench_function("mixed_realistic_workload", |b| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let events = vec![
            AppEvent::Login { username: "testuser".to_string(), password: "password".to_string() },
            AppEvent::AddTask("Buy more milk".to_string()),
            AppEvent::AddTask("Read a book".to_string()),
            AppEvent::CompleteTask(1),
            AppEvent::Sync,
            AppEvent::AddTask("Write some code".to_string()),
            AppEvent::DeleteTask(2),
            AppEvent::SyncWithConflict,
        ];

        b.to_async(rt).iter_batched(
            || {
                // Setup: Create the Syzygy system
                let (core, shell) = Syzygy::builder()
                    .model(UserModel::default())
                    .model(TaskModel::default())
                    .model(AppStateModel::default())
                    .resource(Database {
                        connection_pool: Arc::new(Mutex::new(Vec::new())),
                        query_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(ApiClient {
                        base_url: "http://localhost".to_string(),
                        request_count: Arc::new(AtomicU64::new(0)),
                    })
                    .resource(NotificationService {
                        pending_notifications: Arc::new(Mutex::new(Vec::new())),
                    })
                    .update(update)
                    .build();

                let shell = shell.with_effect_handler(handle_effects);
                let runner = Runner::new(core, shell);
                (runner, events.clone())
            },
            |(mut runner, events)| async move {
                // Action: Run mixed workload
                for event in events {
                    runner.core().send_event(event).unwrap();
                }

                // Tick the runner to process the events
                for _ in 0..10 {
                    if !runner.tick(syzygy::spawn::spawner()).await.unwrap() {
                        break;
                    }
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_memory_efficiency(c: &mut Criterion) {
    c.bench_function("memory_efficiency_1000_tasks", |b| {
        b.iter_with_setup(
            || {
                // Setup: Create 1000 tasks
                let tasks: Vec<Task> = (0..1000)
                    .map(|i| Task {
                        id: i,
                        title: format!("Task {i}"),
                        ..Default::default()
                    })
                    .collect();

                let (core, shell) = Syzygy::builder()
                    .model(UserModel::default())
                    .model(TaskModel {
                        tasks,
                        ..Default::default()
                    })
                    .model(AppStateModel::default())
                    .update(update)
                    .build();

                (core, shell)
            },
            |(core, shell)| {
                // Action: Measure the size of the core and shell
                let core_size = std::mem::size_of_val(&core);
                let shell_size = std::mem::size_of_val(&shell);
                criterion::black_box((core_size, shell_size));
            },
        );
    });
}


criterion_group!(
    benches,
    bench_user_login_flow,
    bench_task_crud_operations,
    bench_bulk_task_operations,
    bench_background_sync,
    bench_mixed_realistic_workload,
    bench_memory_efficiency
);
criterion_main!(benches);
