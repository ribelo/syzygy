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
    ctx: EffectContext<AppEvent, EmptyStorage, AppExecutors>,
) -> syzygy::streaming::EffectOutput<AppEvent> {
    match effect {
        AppEffect::HttpRequest { url } => {
            // Run on the network executor which carries HttpClient as its resource
            let net: &NetExec = ctx.executor::<NetExec, syzygy::storage::Here>();
            let client = net.resource::<HttpClient, syzygy::storage::Here>().clone();
            let tx = ctx.event_sender().expect("event sender available");
            net.spawn(async move {
                match client.get(&url).await {
                    Ok(data) => { let _ = tx.send(AppEvent::HttpResponseReceived { data }); }
                    Err(e) => { let _ = tx.send(AppEvent::NetworkError { message: e }); }
                }
            }).unwrap();
            syzygy::streaming::EffectOutput::None
        }
        AppEffect::DatabaseWrite { value } => {
            // Run on the DB executor which carries Database as its resource
            let db_exec: &DbExec = ctx.executor::<DbExec, syzygy::storage::There<syzygy::storage::Here>>();
            let db = db_exec.resource::<Database, syzygy::storage::Here>().clone();
            let tx = ctx.event_sender().expect("event sender available");
            db_exec.spawn(async move {
                if db.write(value).await.is_ok() { let _ = tx.send(AppEvent::DbWriteOk); }
            }).unwrap();
            syzygy::streaming::EffectOutput::None
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
        .executor(SingleThreadExecutor::new().with_resource(Database))
        .executor(ThreadPerCoreTokioExecutor::new().with_resource(HttpClient))
        .event_handler(update)
        .effect_handler(handle_effects)
        .build();

    let mut runner = Runner::new(core, shell);
    runner.core().send_event(AppEvent::HttpResponseReceived { data: "boot".into() })?;
    runner.run_until(|_, _| true, syzygy::spawn::spawner()).await?;
    Ok(())
}
