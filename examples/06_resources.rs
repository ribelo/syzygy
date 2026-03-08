use std::sync::Arc;
use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
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

fn start(_: ()) -> Command<AppEvent, AppEffect> {
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

/// ## Why request resources in the handler signature?
///
/// Effect handlers automatically inject any requested types that are registered
/// in the `ResourceMap`. Notice `_: ()`: we declare a blank payload because
/// this effect has no data payload, followed by the resources we want.
fn check_services(_payload: (), url: ServerUrl, pool: DatabasePool) -> Task<AppEvent, AppEffect> {
    // We have full access to injected resources!
    let status_msg = format!(
        "Checked {} with {} connections",
        url.0,
        pool.connections.len()
    );

    Task::send(AppEvent::StatusUpdated(status_msg))
}

fn handle_effect(effect: AppEffect, ctx: &EffectContext<'_>) -> Task<AppEvent, AppEffect> {
    match effect {
        // `handle!(check_services, ctx)` extracts the resources from the `EffectContext`
        // and injects them to `check_services`.
        AppEffect::CheckServices => handle!(check_services, ctx),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ## Why `.with_resource`?
    // We register our resources during the builder phase.
    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .with_resource(ServerUrl("https://api.example.com".into()))
        .with_resource(DatabasePool::new())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()?;

    app.core().try_send(AppEvent::Start)?;
    app.step()?;
    app.step()?;

    println!("App Status: {}", app.model().status);
    Ok(())
}
