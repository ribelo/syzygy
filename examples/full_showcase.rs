//! Full showcase of every syzygy feature in one coherent application.
//!
//! A "dashboard" app with two child features (counter + search) composed
//! into a parent via scope, demonstrating:
//!
//! - `#[derive(Model)]` and `#[derive(Resources)]` proc macros
//! - Magic event handlers (per-field extraction via `FromEventContext`)
//! - Magic effect handlers (resource extraction via `FromEffectContext`)
//! - `AsyncRt` extraction in effect handlers (runtime-agnostic sleep/delay)
//! - Scope composition (`EventContext::scope`, `EffectContext::scope`)
//! - Command mapping (`map`, `map_event`, `map_effect`)
//! - Cancellable effects (`Command::cancellable`, `CancelId`)
//! - Command builders (`none`, `event`, `events`, `effect`, `sequential`,
//!   `parallel`, `batch`, `and`, `and_event`, `and_effect`, `and_cancellable`)
//! - Free-function builders (`cmd::effect`, `cmd::cancellable`)
//! - Reducer composition (`Reduce`, `scope`, `combine`, `boxed`)
//! - `TestStore` (`send`, `state`, `assert_effects`, `assert_no_effects`,
//!   `take_effects`, `with_max_event_steps`)
//! - Panic testing (`assert_panic_contains`)
//! - Semantic Task variants (`event`, `events`, `none`, `future`,
//!   `compute`, `blocking`) — handlers declare work shape, Shell routes
//! - Task mapping (`map`, `map_event`, `map_effect`)
//! - Full `Syzygy` runtime (builder, model, resources, handlers,
//!   reducer, executor, build, step, shutdown)

use syzygy::prelude::*;

// ════════════════════════════════════════════════════════════════════
// Child Feature: Counter
// ════════════════════════════════════════════════════════════════════

mod counter {
    use syzygy::prelude::*;

    // #[derive(Model)] generates:
    //   pub struct Value(*mut i32)        + Deref/DerefMut + FromEventContext<State>
    //   pub struct History(*mut Vec<i32>) + Deref/DerefMut + FromEventContext<State>
    #[derive(Debug, Default, Model)]
    pub struct State {
        pub value: i32,
        pub history: Vec<i32>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[allow(dead_code)]
    pub enum Event {
        Increment(i32),
        Decrement(i32),
        Reset,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Effect {
        LogValue(i32),
        ComputeChecksum(Vec<i32>),
    }

    // #[derive(Resources)] generates:
    //   pub struct LogPrefix(pub String) + FromEffectContext<Res>
    #[derive(Debug, Clone, Resources)]
    pub struct Res {
        pub log_prefix: String,
    }

    // Magic event handlers: payload first, then extracted model fields.

    fn increment(amount: i32, mut value: Value, mut history: History) -> Command<Event, Effect> {
        *value += amount;
        history.push(*value);
        let mut cmd = Command::effect(Effect::LogValue(*value));
        if history.len() >= 3 {
            cmd = cmd.and_effect(Effect::ComputeChecksum(history.clone()));
        }
        cmd
    }

    fn decrement(amount: i32, mut value: Value) -> Command<Event, Effect> {
        *value -= amount;
        Command::effect(Effect::LogValue(*value))
    }

    fn reset(_: (), mut value: Value, mut history: History) -> Command<Event, Effect> {
        *value = 0;
        history.clear();
        Command::none()
    }

    pub fn dispatch(event: Event, ctx: &EventContext<State>) -> Command<Event, Effect> {
        match event {
            Event::Increment(n) => increment.handle(n, ctx),
            Event::Decrement(n) => decrement.handle(n, ctx),
            Event::Reset => reset.handle((), ctx),
        }
    }

    // Magic effect handler: payload first, then extracted resources.

    fn log_value(value: i32, prefix: LogPrefix) -> Task<Event, Effect> {
        // Task::blocking — runs on the blocking executor (spawn_blocking).
        // Use for IO-bound work that would block the async runtime:
        // file writes, synchronous HTTP, database queries, etc.
        Task::blocking(move || {
            println!("[{}] counter = {value}", prefix.0);
            Command::none()
        })
    }

    fn compute_checksum(values: Vec<i32>) -> Task<Event, Effect> {
        // Task::compute — runs on the compute executor (e.g. Rayon).
        // Use for CPU-bound work: hashing, sorting, compression, etc.
        // Shell routes this to compute_executor, falls back to blocking_executor.
        Task::compute(move || {
            let checksum: i32 = values.iter().fold(0, |acc, &v| acc ^ v);
            Command::event(Event::Increment(checksum % 10))
        })
    }

    pub fn dispatch_effect(effect: Effect, ctx: &EffectContext<Res>) -> Task<Event, Effect> {
        match effect {
            Effect::LogValue(v) => log_value.handle(v, ctx),
            Effect::ComputeChecksum(values) => compute_checksum.handle(values, ctx),
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// Child Feature: Search (with cancellable effects)
// ════════════════════════════════════════════════════════════════════

mod search {
    use std::time::Duration;

    use syzygy::prelude::*;

    // #[derive(Model)] generates: Query, Results, Loading wrappers
    #[derive(Debug, Default, Model)]
    pub struct State {
        pub query: String,
        pub results: Vec<String>,
        pub loading: bool,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Event {
        UpdateQuery(String),
        ResultsLoaded(Vec<String>),
        Clear,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    #[allow(dead_code)]
    pub enum Effect {
        ExecuteSearch(String),
        DebouncedSearch { query: String, delay: Duration },
    }

    // #[derive(Resources)] generates: ApiUrl(pub String) wrapper
    #[derive(Debug, Clone, Resources)]
    pub struct Res {
        pub api_url: String,
    }

    fn update_query(
        new_query: String,
        mut query: Query,
        mut loading: Loading,
    ) -> Command<Event, Effect> {
        (*query).clone_from(&new_query);
        *loading = true;
        // Cancellable: a new search with the same ID cancels the previous in-flight one.
        Command::cancellable(
            "search",
            Effect::DebouncedSearch {
                query: new_query,
                delay: Duration::from_millis(300),
            },
        )
    }

    fn results_loaded(
        new_results: Vec<String>,
        mut results: Results,
        mut loading: Loading,
    ) -> Command<Event, Effect> {
        *results = new_results;
        *loading = false;
        Command::none()
    }

    fn clear(
        _: (),
        mut query: Query,
        mut results: Results,
        mut loading: Loading,
    ) -> Command<Event, Effect> {
        *query = String::new();
        results.clear();
        *loading = false;
        Command::none()
    }

    pub fn dispatch(event: Event, ctx: &EventContext<State>) -> Command<Event, Effect> {
        match event {
            Event::UpdateQuery(q) => update_query.handle(q, ctx),
            Event::ResultsLoaded(r) => results_loaded.handle(r, ctx),
            Event::Clear => clear.handle((), ctx),
        }
    }

    // AsyncRt — extracted from EffectContext, provides runtime-agnostic
    // async capabilities (sleep, timeout). The handler never names a
    // specific runtime (tokio, smol, etc.) — switching runtimes changes
    // zero handler code.
    fn debounced_search(
        payload: (String, Duration),
        api_url: ApiUrl,
        rt: AsyncRt,
    ) -> Task<Event, Effect> {
        let (query, delay) = payload;
        if query.is_empty() {
            return Task::events(vec![Event::ResultsLoaded(vec![])]);
        }
        // Task::future — async IO work on the configured async executor.
        let sleep_fut = rt.sleep(delay);
        Task::future(async move {
            sleep_fut.await;
            let results = vec![
                format!("{query} from {}", api_url.0),
                format!("{query} - result 2"),
            ];
            Command::event(Event::ResultsLoaded(results))
        })
    }

    fn execute_search(query: String, api_url: ApiUrl) -> Task<Event, Effect> {
        if query.is_empty() {
            return Task::events(vec![Event::ResultsLoaded(vec![])]);
        }
        Task::future(async move {
            let results = vec![
                format!("{query} from {}", api_url.0),
                format!("{query} - result 2"),
            ];
            Command::event(Event::ResultsLoaded(results))
        })
    }

    pub fn dispatch_effect(effect: Effect, ctx: &EffectContext<Res>) -> Task<Event, Effect> {
        match effect {
            Effect::ExecuteSearch(q) => execute_search.handle(q, ctx),
            Effect::DebouncedSearch { query, delay } => {
                debounced_search.handle((query, delay), ctx)
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════
// Parent App — composes counter + search via scope
// ════════════════════════════════════════════════════════════════════

// #[derive(Model)] generates: Counter, Search, Status wrappers
// (Counter/Search give direct *mut access to child state; Status for the String field)
#[derive(Debug, Default, Model)]
struct AppState {
    pub counter: counter::State,
    pub search: search::State,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    Counter(counter::Event),
    Search(search::Event),
    SetStatus(String),
    Initialize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {
    Counter(counter::Effect),
    Search(search::Effect),
    Audit(String),
    BlockingAudit(String),
}

// #[derive(Resources)] generates: ApiUrl, LogPrefix wrappers
#[derive(Debug, Clone, Resources)]
struct AppRes {
    pub api_url: String,
    pub log_prefix: String,
}

// ── Event dispatch — manual scope composition ───────────────────────

fn dispatch_event(event: AppEvent, ctx: &EventContext<AppState>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Counter(e) => {
            // EventContext::scope — project parent model into child context.
            let child = ctx.scope(|m| &mut m.counter);
            // Command::map — transform both event and effect types at once.
            counter::dispatch(e, &child).map(AppEvent::Counter, AppEffect::Counter)
        }
        AppEvent::Search(e) => {
            let child = ctx.scope(|m| &mut m.search);
            // Command::map_event + map_effect — transform types separately.
            search::dispatch(e, &child)
                .map_event(AppEvent::Search)
                .map_effect(AppEffect::Search)
        }
        AppEvent::SetStatus(s) => {
            // FromEventContext extraction at the parent level via derived Status wrapper.
            let mut status = Status::from_context(ctx);
            (*status).clone_from(&s);
            // cmd::effect — free-function builder from prelude.
            cmd::effect(AppEffect::Audit(format!("status: {s}")))
        }
        AppEvent::Initialize => {
            // Demonstrates: events, and_event, and_effect, and (chaining),
            // sequential, parallel, and_cancellable — all in one command.
            Command::events([
                AppEvent::SetStatus("ready".into()),
                AppEvent::Counter(counter::Event::Reset),
                AppEvent::Search(search::Event::Clear),
            ])
            .and_event(AppEvent::Counter(counter::Event::Increment(1)))
            .and_effect(AppEffect::Audit("init-done".into()))
            .and(Command::sequential([
                AppEffect::Audit("seq-1".into()),
                AppEffect::Audit("seq-2".into()),
            ]))
            .and(Command::parallel([
                AppEffect::Counter(counter::Effect::LogValue(0)),
                AppEffect::Audit("par-a".into()),
            ]))
            .and_cancellable(42_u64, AppEffect::Audit("startup-check".into()))
            .and_effect(AppEffect::BlockingAudit("init-log".into()))
        }
    }
}

// ── Effect dispatch — scope composition for resources ───────────────

fn dispatch_effect(effect: AppEffect, ctx: &EffectContext<AppRes>) -> Task<AppEvent, AppEffect> {
    match effect {
        AppEffect::Counter(e) => {
            // EffectContext::scope — derive child resources from parent.
            let child = ctx.scope(|r| counter::Res {
                log_prefix: r.log_prefix.clone(),
            });
            // Task::map_event + Task::map_effect — remap child types.
            counter::dispatch_effect(e, &child)
                .map_event(AppEvent::Counter)
                .map_effect(AppEffect::Counter)
        }
        AppEffect::Search(e) => {
            let child = ctx.scope(|r| search::Res {
                api_url: r.api_url.clone(),
            });
            // Task::map — remap both types at once.
            search::dispatch_effect(e, &child).map(AppEvent::Search, AppEffect::Search)
        }
        AppEffect::Audit(msg) => {
            let prefix = LogPrefix::from_context(ctx);
            println!("[{}] AUDIT: {msg}", prefix.0);
            Task::none()
        }
        AppEffect::BlockingAudit(msg) => {
            let prefix = LogPrefix::from_context(ctx);
            // Task::blocking — offload to blocking executor (spawn_blocking).
            Task::blocking(move || {
                println!("[{}] BLOCKING-AUDIT: {msg}", prefix.0);
                Command::none()
            })
        }
    }
}

// ── Reducer-based composition (alternative to manual dispatch) ──────

#[cfg(test)]
fn build_app_reducer() -> impl Reducer<State = AppState, Event = AppEvent, Effect = AppEffect> + Send
{
    // Reduce::new wraps a dispatch function into the Reducer trait.
    let counter_reducer = Reduce::new(counter::dispatch);
    let search_reducer = Reduce::new(search::dispatch);

    // Reducer::scope — lift a child reducer to operate on parent types.
    let scoped_counter = counter_reducer.scope(
        |s: &mut AppState| &mut s.counter,
        |e: AppEvent| match e {
            AppEvent::Counter(e) => Some(e),
            _ => None,
        },
        AppEvent::Counter,
        AppEffect::Counter,
    );

    let scoped_search = search_reducer.scope(
        |s: &mut AppState| &mut s.search,
        |e: AppEvent| match e {
            AppEvent::Search(e) => Some(e),
            _ => None,
        },
        AppEvent::Search,
        AppEffect::Search,
    );

    // App-local reducer for events that don't route to children.
    let app_local = Reduce::new(
        |event: AppEvent, ctx: &EventContext<AppState>| match event {
            AppEvent::SetStatus(s) => {
                let mut status = Status::from_context(ctx);
                *status = s;
                Command::none()
            }
            AppEvent::Initialize => Command::events([AppEvent::SetStatus("ready".into())]),
            _ => Command::none(),
        },
    );

    // combine + boxed — merge multiple reducers; each gets every event.
    combine(vec![
        scoped_counter.boxed(),
        scoped_search.boxed(),
        app_local.boxed(),
    ])
}

// ════════════════════════════════════════════════════════════════════
// main — full Syzygy runtime
// ════════════════════════════════════════════════════════════════════

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppState::default())
        .with_resources(AppRes {
            api_url: "https://api.example.test".into(),
            log_prefix: "showcase".into(),
        })
        .event_handler(dispatch_event)
        .effect_handler(dispatch_effect)
        .async_executor(InlineAsync::new())
        .blocking_executor(InlineBlocking::new())
        .build();

    // Initialize the app.
    runner.core().try_send_event(AppEvent::Initialize)?;
    runner.step()?;
    runner.step()?;

    // Increment counter.
    runner
        .core()
        .try_send_event(AppEvent::Counter(counter::Event::Increment(5)))?;
    runner.step()?;

    // Debounced search: second query cancels the first.
    runner
        .core()
        .try_send_event(AppEvent::Search(search::Event::UpdateQuery("hello".into())))?;
    runner.step()?;
    runner
        .core()
        .try_send_event(AppEvent::Search(search::Event::UpdateQuery("world".into())))?;
    runner.step()?;
    runner.step()?;

    let m = runner.model();
    println!(
        "counter: {} (history: {:?})",
        m.counter.value, m.counter.history
    );
    println!(
        "search: query='{}', results={:?}, loading={}",
        m.search.query, m.search.results, m.search.loading
    );
    println!("status: {}", m.status);

    runner.shutdown();
    Ok(())
}

// ════════════════════════════════════════════════════════════════════
// Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    // ── Counter child ─────────────────────────────────────────────

    #[test]
    fn counter_increment_updates_value_and_history() {
        let mut store = TestStore::new(counter::State::default(), counter::dispatch);

        store.send(counter::Event::Increment(3));

        assert_eq!(store.state().value, 3);
        assert_eq!(store.state().history, vec![3]);
        store.assert_effects([counter::Effect::LogValue(3)]);
    }

    #[test]
    fn counter_decrement() {
        let mut store = TestStore::new(
            counter::State {
                value: 10,
                history: vec![10],
            },
            counter::dispatch,
        );

        store.send(counter::Event::Decrement(3));

        assert_eq!(store.state().value, 7);
        store.assert_effects([counter::Effect::LogValue(7)]);
    }

    #[test]
    fn counter_reset_clears_state() {
        let mut store = TestStore::new(
            counter::State {
                value: 5,
                history: vec![1, 5],
            },
            counter::dispatch,
        );

        store.send(counter::Event::Reset);

        assert_eq!(store.state().value, 0);
        assert!(store.state().history.is_empty());
        store.assert_no_effects();
    }

    #[test]
    fn counter_double_borrow_panics() {
        fn bad(
            _: (),
            _v1: counter::Value,
            _v2: counter::Value,
        ) -> Command<counter::Event, counter::Effect> {
            unreachable!()
        }

        let mut store = TestStore::new(
            counter::State::default(),
            |event: counter::Event, ctx: &EventContext<counter::State>| match event {
                counter::Event::Increment(_) => bad.handle((), ctx),
                other => counter::dispatch(other, ctx),
            },
        );

        assert_panic_contains("already borrowed mutably", || {
            store.send(counter::Event::Increment(1));
        });
    }

    // ── Search child ──────────────────────────────────────────────

    #[test]
    fn search_update_query_emits_cancellable_effect() {
        let mut store = TestStore::new(search::State::default(), search::dispatch);

        store.send(search::Event::UpdateQuery("test".into()));

        assert_eq!(store.state().query, "test");
        assert!(store.state().loading);
        store.assert_effects([search::Effect::DebouncedSearch {
            query: "test".into(),
            delay: Duration::from_millis(300),
        }]);
    }

    #[test]
    fn search_results_loaded_clears_loading() {
        let mut store = TestStore::new(
            search::State {
                query: "q".into(),
                results: vec![],
                loading: true,
            },
            search::dispatch,
        );

        store.send(search::Event::ResultsLoaded(vec!["a".into(), "b".into()]));

        assert!(!store.state().loading);
        assert_eq!(store.state().results, vec!["a", "b"]);
        store.assert_no_effects();
    }

    #[test]
    fn search_clear_resets_everything() {
        let mut store = TestStore::new(
            search::State {
                query: "x".into(),
                results: vec!["y".into()],
                loading: true,
            },
            search::dispatch,
        );

        store.send(search::Event::Clear);

        assert_eq!(store.state().query, "");
        assert!(store.state().results.is_empty());
        assert!(!store.state().loading);
        store.assert_no_effects();
    }

    // ── App composition (manual dispatch with scope) ──────────────

    #[test]
    fn app_routes_counter_via_scope() {
        let mut store = TestStore::new(AppState::default(), dispatch_event);

        store.send(AppEvent::Counter(counter::Event::Increment(4)));

        assert_eq!(store.state().counter.value, 4);
        assert_eq!(store.state().counter.history, vec![4]);
        store.assert_effects([AppEffect::Counter(counter::Effect::LogValue(4))]);
    }

    #[test]
    fn app_routes_search_via_scope() {
        let mut store = TestStore::new(AppState::default(), dispatch_event);

        store.send(AppEvent::Search(search::Event::UpdateQuery("hello".into())));

        assert_eq!(store.state().search.query, "hello");
        assert!(store.state().search.loading);
        store.assert_effects([AppEffect::Search(search::Effect::DebouncedSearch {
            query: "hello".into(),
            delay: Duration::from_millis(300),
        })]);
    }

    #[test]
    fn app_set_status_uses_derived_extractor_and_cmd_builder() {
        let mut store = TestStore::new(AppState::default(), dispatch_event);

        store.send(AppEvent::SetStatus("active".into()));

        assert_eq!(store.state().status, "active");
        store.assert_effects([AppEffect::Audit("status: active".into())]);
    }

    #[test]
    fn app_initialize_demonstrates_all_command_builders() {
        let mut store = TestStore::new(
            AppState {
                counter: counter::State {
                    value: 99,
                    history: vec![99],
                },
                ..AppState::default()
            },
            dispatch_event,
        );

        store.send(AppEvent::Initialize);

        // Chained events: SetStatus("ready"), Counter::Reset, Search::Clear,
        // Counter::Increment(1) — all processed synchronously.
        assert_eq!(store.state().status, "ready");
        assert_eq!(store.state().counter.value, 1);
        assert_eq!(store.state().counter.history, vec![1]);
        assert!(store.state().search.results.is_empty());
        assert!(!store.state().search.loading);

        let effects = store.take_effects();
        assert_eq!(
            effects,
            vec![
                AppEffect::Audit("init-done".into()),
                AppEffect::Audit("seq-1".into()),
                AppEffect::Audit("seq-2".into()),
                AppEffect::Counter(counter::Effect::LogValue(0)),
                AppEffect::Audit("par-a".into()),
                AppEffect::Audit("startup-check".into()),
                AppEffect::BlockingAudit("init-log".into()),
                AppEffect::Audit("status: ready".into()),
                AppEffect::Counter(counter::Effect::LogValue(1)),
            ]
        );
    }

    // ── App-level derived Model extractors ────────────────────────

    #[test]
    fn app_derived_model_extractors_give_direct_child_access() {
        // #[derive(Model)] on AppState also generates Counter/Search/Status
        // wrappers. Counter derefs to &counter::State, etc.
        let mut store = TestStore::new(
            AppState {
                counter: counter::State {
                    value: 42,
                    history: vec![42],
                },
                ..AppState::default()
            },
            |event: AppEvent, ctx: &EventContext<AppState>| {
                if let AppEvent::SetStatus(s) = event {
                    let counter_access = Counter::from_context(ctx);
                    let _search_access = Search::from_context(ctx);
                    let mut status = Status::from_context(ctx);
                    *status = format!("{s} (counter={})", counter_access.value);
                    Command::<AppEvent, AppEffect>::none()
                } else {
                    Command::<AppEvent, AppEffect>::none()
                }
            },
        );

        store.send(AppEvent::SetStatus("snapshot".into()));

        assert_eq!(store.state().status, "snapshot (counter=42)");
    }

    // ── Effect scope ──────────────────────────────────────────────

    #[test]
    fn effect_scope_routes_to_counter_child() {
        let ctx = EffectContext::new(AppRes {
            api_url: "https://test.api".into(),
            log_prefix: "test".into(),
        });
        let child_ctx = ctx.scope(|r| counter::Res {
            log_prefix: r.log_prefix.clone(),
        });

        let task = counter::dispatch_effect(counter::Effect::LogValue(42), &child_ctx);

        match task {
            Task::Blocking { job, .. } => {
                let cmd = job();
                assert_eq!(cmd.into_iter().count(), 0);
            }
            _ => panic!("expected Task::Blocking from log_value"),
        }
    }

    #[test]
    fn effect_scope_routes_to_search_child() {
        let ctx = EffectContext::new(AppRes {
            api_url: "https://test.api".into(),
            log_prefix: "test".into(),
        });
        let child_ctx = ctx.scope(|r| search::Res {
            api_url: r.api_url.clone(),
        });

        let task = search::dispatch_effect(search::Effect::ExecuteSearch("".into()), &child_ctx);

        // Empty query returns Task::events([ResultsLoaded(vec![])]).
        match task {
            Task::Events(events) => {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0], search::Event::ResultsLoaded(vec![]));
            }
            _ => panic!("expected Task::Events for empty query"),
        }
    }

    // ── Semantic Task variants ───────────────────────────────────

    #[test]
    fn task_compute_runs_cpu_bound_work() {
        let ctx = EffectContext::new(counter::Res {
            log_prefix: "test".into(),
        });

        let task = counter::dispatch_effect(counter::Effect::ComputeChecksum(vec![1, 2, 3]), &ctx);

        match task {
            Task::Compute { job, .. } => {
                let cmd = job();
                let events: Vec<_> = cmd
                    .into_iter()
                    .filter_map(|s| {
                        if let CommandStep::Event(e) = s {
                            Some(e)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(events.len(), 1);
            }
            _ => panic!("expected Task::Compute from compute_checksum"),
        }
    }

    #[test]
    fn task_blocking_runs_io_work() {
        let ctx = EffectContext::new(counter::Res {
            log_prefix: "test".into(),
        });

        let task = counter::dispatch_effect(counter::Effect::LogValue(7), &ctx);

        match task {
            Task::Blocking { job, .. } => {
                let cmd = job();
                assert_eq!(cmd.into_iter().count(), 0);
            }
            _ => panic!("expected Task::Blocking from log_value"),
        }
    }

    #[test]
    fn task_future_with_async_rt_debounce() {
        let ctx = EffectContext::with_async_executor(
            search::Res {
                api_url: "https://test.api".into(),
            },
            Some(std::sync::Arc::new(InlineAsync::new())),
        );

        let task = search::dispatch_effect(
            search::Effect::DebouncedSearch {
                query: "hello".into(),
                delay: Duration::from_millis(10),
            },
            &ctx,
        );

        match task {
            Task::Async { future, .. } => {
                let cmd = futures::executor::block_on(future);
                let events: Vec<_> = cmd
                    .into_iter()
                    .filter_map(|s| {
                        if let CommandStep::Event(e) = s {
                            Some(e)
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(events.len(), 1);
                assert_eq!(
                    events[0],
                    search::Event::ResultsLoaded(vec![
                        "hello from https://test.api".into(),
                        "hello - result 2".into(),
                    ])
                );
            }
            _ => panic!("expected Task::Async from debounced_search"),
        }
    }

    #[test]
    fn counter_triggers_compute_after_three_increments() {
        let mut store = TestStore::new(counter::State::default(), counter::dispatch);

        store.send(counter::Event::Increment(1));
        store.send(counter::Event::Increment(2));
        let effects = store.take_effects();
        assert_eq!(effects.len(), 2);
        assert!(!effects
            .iter()
            .any(|e| matches!(e, counter::Effect::ComputeChecksum(_))));

        store.send(counter::Event::Increment(3));
        let effects = store.take_effects();
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[0], counter::Effect::LogValue(6));
        assert!(matches!(&effects[1], counter::Effect::ComputeChecksum(v) if v.len() == 3));
    }

    // ── Reducer composition ───────────────────────────────────────

    #[test]
    fn reducer_routes_counter_via_scope() {
        let reducer = build_app_reducer();
        let mut store = TestStore::new(AppState::default(), move |event, ctx| {
            reducer.reduce(event, ctx)
        });

        store.send(AppEvent::Counter(counter::Event::Increment(7)));

        assert_eq!(store.state().counter.value, 7);
        store.assert_effects([AppEffect::Counter(counter::Effect::LogValue(7))]);
    }

    #[test]
    fn reducer_routes_search_via_scope() {
        let reducer = build_app_reducer();
        let mut store = TestStore::new(AppState::default(), move |event, ctx| {
            reducer.reduce(event, ctx)
        });

        store.send(AppEvent::Search(search::Event::UpdateQuery("q".into())));

        assert_eq!(store.state().search.query, "q");
        store.assert_effects([AppEffect::Search(search::Effect::DebouncedSearch {
            query: "q".into(),
            delay: Duration::from_millis(300),
        })]);
    }

    #[test]
    fn reducer_combine_handles_local_events() {
        let reducer = build_app_reducer();
        let mut store = TestStore::new(AppState::default(), move |event, ctx| {
            reducer.reduce(event, ctx)
        });

        store.send(AppEvent::SetStatus("hello".into()));

        assert_eq!(store.state().status, "hello");
        store.assert_no_effects();
    }

    #[test]
    fn reducer_ignores_unmatched_events_in_child_scopes() {
        let reducer = build_app_reducer();
        let mut store = TestStore::new(AppState::default(), move |event, ctx| {
            reducer.reduce(event, ctx)
        });

        store.send(AppEvent::Initialize);

        // Only app_local handles Initialize -> chains SetStatus("ready").
        assert_eq!(store.state().status, "ready");
    }

    // ── TestStore advanced ────────────────────────────────────────

    #[test]
    fn test_store_with_max_event_steps_guard() {
        let store =
            TestStore::new(counter::State::default(), counter::dispatch).with_max_event_steps(100);

        assert_eq!(store.state().value, 0);
    }

    #[test]
    fn test_store_take_effects_drains_buffer() {
        let mut store = TestStore::new(AppState::default(), dispatch_event);

        store.send(AppEvent::Counter(counter::Event::Increment(3)));
        store.send(AppEvent::Counter(counter::Event::Increment(2)));

        let effects = store.take_effects();
        assert_eq!(effects.len(), 2);
        assert_eq!(effects[0], AppEffect::Counter(counter::Effect::LogValue(3)));
        assert_eq!(effects[1], AppEffect::Counter(counter::Effect::LogValue(5)));
        store.assert_no_effects();
    }

    #[test]
    fn test_store_manual_effect_round_trip() {
        let mut store = TestStore::new(AppState::default(), dispatch_event);
        let effect_ctx = EffectContext::with_async_executor(
            AppRes {
                api_url: "https://test.api".into(),
                log_prefix: "test".into(),
            },
            Some(std::sync::Arc::new(InlineAsync::new())),
        );

        store.send(AppEvent::Search(search::Event::UpdateQuery("rust".into())));
        let effects = store.take_effects();
        assert_eq!(effects.len(), 1);

        fn feed_task(
            store: &mut TestStore<AppEvent, AppEffect, AppState>,
            task: Task<AppEvent, AppEffect>,
        ) {
            match task {
                Task::Async { future, .. } => {
                    let cmd = futures::executor::block_on(future);
                    for step in cmd {
                        if let CommandStep::Event(e) = step {
                            store.send(e);
                        }
                    }
                }
                Task::Blocking { job, .. } => {
                    let cmd = job();
                    for step in cmd {
                        if let CommandStep::Event(e) = step {
                            store.send(e);
                        }
                    }
                }
                Task::Compute { job, .. } => {
                    let cmd = job();
                    for step in cmd {
                        if let CommandStep::Event(e) = step {
                            store.send(e);
                        }
                    }
                }
                Task::Event(e) => store.send(e),
                Task::Events(events) => {
                    for e in events {
                        store.send(e);
                    }
                }
                _ => {}
            }
        }

        for effect in effects {
            let task = dispatch_effect(effect, &effect_ctx);
            feed_task(&mut store, task);
        }

        assert!(!store.state().search.loading);
        assert_eq!(store.state().search.results.len(), 2);
        assert!(store.state().search.results[0].contains("rust"));
    }

    // ── Full Syzygy runtime ───────────────────────────────────────

    #[test]
    fn full_runtime_with_manual_dispatch() {
        let mut runner = Syzygy::builder::<AppEvent, AppEffect>()
            .model(AppState::default())
            .with_resources(AppRes {
                api_url: "https://test.api".into(),
                log_prefix: "rt".into(),
            })
            .event_handler(dispatch_event)
            .effect_handler(dispatch_effect)
            .async_executor(InlineAsync::new())
            .blocking_executor(InlineBlocking::new())
            .build();

        runner
            .core()
            .try_send_event(AppEvent::Counter(counter::Event::Increment(10)))
            .unwrap();
        runner.step().unwrap();

        assert_eq!(runner.model().counter.value, 10);
        assert_eq!(runner.model().counter.history, vec![10]);

        runner.shutdown();
    }

    #[test]
    fn full_runtime_with_reducer() {
        let reducer = build_app_reducer();

        let mut runner = Syzygy::builder::<AppEvent, AppEffect>()
            .model(AppState::default())
            .with_resources(AppRes {
                api_url: "https://test.api".into(),
                log_prefix: "rt".into(),
            })
            .reducer(reducer)
            .effect_handler(dispatch_effect)
            .async_executor(InlineAsync::new())
            .blocking_executor(InlineBlocking::new())
            .build();

        runner
            .core()
            .try_send_event(AppEvent::Counter(counter::Event::Increment(7)))
            .unwrap();
        runner.step().unwrap();

        assert_eq!(runner.model().counter.value, 7);

        runner.shutdown();
    }

    #[test]
    fn full_runtime_search_with_async_effect() {
        let mut runner = Syzygy::builder::<AppEvent, AppEffect>()
            .model(AppState::default())
            .with_resources(AppRes {
                api_url: "https://test.api".into(),
                log_prefix: "rt".into(),
            })
            .event_handler(dispatch_event)
            .effect_handler(dispatch_effect)
            .async_executor(InlineAsync::new())
            .blocking_executor(InlineBlocking::new())
            .build();

        runner
            .core()
            .try_send_event(AppEvent::Search(search::Event::UpdateQuery("tokio".into())))
            .unwrap();
        // Step 1: process UpdateQuery -> queues ExecuteSearch effect.
        runner.step().unwrap();
        // Step 2: Shell runs async effect -> produces ResultsLoaded event.
        runner.step().unwrap();
        // Step 3: Core processes ResultsLoaded.
        runner.step().unwrap();

        assert_eq!(runner.model().search.query, "tokio");
        assert!(!runner.model().search.loading);
        assert!(runner.model().search.results[0].contains("tokio"));

        runner.shutdown();
    }
}
