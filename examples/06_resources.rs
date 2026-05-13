use std::sync::Arc;
use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
    #[model(wrapper = Status)]
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    Start,
    StatusUpdated(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {
    CheckServices,
}

fn start() -> Command<AppEvent, AppEffect> {
    Command::effect(AppEffect::CheckServices)
}

fn status_updated(msg: String, status: &mut Status) -> Command<AppEvent, AppEffect> {
    **status = msg;
    Command::none()
}

fn handle_event(event: AppEvent, ctx: &EventContext<AppModel>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Start => handle!(start, ctx),
        AppEvent::StatusUpdated(msg) => handle!(status_updated, ctx, msg),
    }
}

// ==========================================
// Resources Configuration
// ==========================================

/// ## Why a cheap clone resource?
///
/// Types registered as resources must implement `Clone`. Cheaply cloneable types
/// (like `String` or small data types) can be requested directly.
#[derive(Clone)]
struct ServerUrl(String);

/// ## Why wrap in `Arc` for expensive resources?
///
/// Effect Handlers clone the resource out of the context every time they run.
/// If a resource is expensive to clone (like a connection pool),
/// it should be wrapped in an `Arc` for minimal overhead.
#[derive(Clone)]
struct DatabasePool {
    connections: Arc<[String]>,
}

impl DatabasePool {
    fn new() -> Self {
        Self {
            connections: Arc::<[String]>::from(vec!["conn1".into(), "conn2".into()]),
        }
    }
}

#[derive(Clone)]
struct AppResources {
    server_url: ServerUrl,
    database_pool: DatabasePool,
}

impl FromEffectContext<AppResources> for ServerUrl {
    fn from_context(ctx: &EffectContext<'_, AppResources>) -> Self {
        ctx.state().server_url.clone()
    }
}

impl FromEffectContext<AppResources> for DatabasePool {
    fn from_context(ctx: &EffectContext<'_, AppResources>) -> Self {
        ctx.state().database_pool.clone()
    }
}

/// ## Why keep resource work separate?
///
/// `check_services` only needs the resources themselves. The surrounding effect
/// handler decides how to extract them from the `EffectContext`.
fn check_services(url: ServerUrl, pool: DatabasePool) -> Task<AppEvent, AppEffect> {
    // We have full access to injected resources!
    let status_msg = format!(
        "Checked {} with {} connections",
        url.0,
        pool.connections.len()
    );

    Task::send(AppEvent::StatusUpdated(status_msg))
}

fn handle_effect(
    effect: AppEffect,
    ctx: &EffectContext<'_, AppResources>,
) -> Task<AppEvent, AppEffect> {
    match effect {
        AppEffect::CheckServices => {
            let url = ServerUrl::from_context(ctx);
            let pool = DatabasePool::from_context(ctx);
            check_services(url, pool)
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ## Why `.resources`?
    // We pass one user-owned resources struct during the builder phase.
    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .resources(AppResources {
            server_url: ServerUrl("https://api.example.com".into()),
            database_pool: DatabasePool::new(),
        })
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()?;

    app.core().emit(AppEvent::Start);
    app.step()?;
    app.step()?;

    println!("App Status: {}", app.model().status);
    Ok(())
}
