use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum AppEvent {
    HttpResponseReceived { data: String },
    DbWriteOk,
    NetworkError { message: String },
}

#[derive(Debug, Clone)]
enum AppEffect {
    HttpRequest { url: String },
    DatabaseWrite { value: i32 },
}

#[derive(Default)]
struct Model;

#[derive(Clone)]
struct HttpClient;
impl HttpClient { async fn get(&self, _u: &str) -> Result<String, String> { Ok("ok".into()) } }

#[derive(Clone)]
struct Database;
impl Database { async fn write(&self, _v: i32) -> Result<(), String> { Ok(()) } }

// Resources used by executors
type NetResources = Storage<HttpClient, EmptyStorage>;
type DbResources = Storage<Database, EmptyStorage>;

// Executor types with their own resources
type NetExec = ThreadPerCoreTokioExecutor<AppEvent, NetResources>;
type DbExec = SingleThreadExecutor<AppEvent, DbResources>;

// Executor storage chain (order matches builder: last-added is the head)
type AppExecutors = syzygy::executor::ExecutorStorage<
    NetExec,
    syzygy::executor::ExecutorStorage<DbExec, syzygy::executor::EmptyExecutorStorage>,
>;

// Single effect handler using multi-executor routing
async fn handle_effects(
    effect: AppEffect,
    ctx: EffectContext<AppEvent>,
) -> syzygy::streaming::EffectResult<AppEvent> {
    match effect {
        AppEffect::HttpRequest { url } => {
            // Use resource directly from context
            let client: &HttpClient = ctx.resource().expect("HttpClient should be available");
            let client_clone = client.clone();
            let task = async move {
                match client_clone.get(&url).await {
                    Ok(data) => { let _ = ctx.send_event(AppEvent::HttpResponseReceived { data }); }
                    Err(e) => { let _ = ctx.send_event(AppEvent::NetworkError { message: e }); }
                }
            };
            ctx.spawn(task).unwrap();
            syzygy::streaming::EffectResult::None
        }
        AppEffect::DatabaseWrite { value } => {
            // Use resource directly from context
            let db: &Database = ctx.resource().expect("Database should be available");
            let db_clone = db.clone();
            let task = async move {
                if db_clone.write(value).await.is_ok() {
                    let _ = ctx.send_event(AppEvent::DbWriteOk);
                }
            };
            ctx.spawn(task).unwrap();
            syzygy::streaming::EffectResult::None
        }
    }
}

// No newtype wrappers needed; distinct executor types can coexist in storage

fn update(event: AppEvent, _ctx: &mut EventContext<AppEvent, AppEffect, Storage<Model, EmptyStorage>>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::HttpResponseReceived { .. } | AppEvent::DbWriteOk | AppEvent::NetworkError { .. } => Command::none(),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (core, shell) = Syzygy::builder::<AppEvent, AppEffect>()
        .model(Model)
        // Attach resources to the executors that use them
        .event_handler(update)
        .effect_handler(handle_effects)
        .build();

    let mut runner = Runner::new(core, shell);
    runner.core().send_event(AppEvent::HttpResponseReceived { data: "boot".into() })?;
    runner.run_until(|_, _| true, syzygy::spawn::spawner()).await?;
    Ok(())
}
