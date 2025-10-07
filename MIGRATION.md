# Migration Guide

This guide highlights the breaking changes in the new Syzygy runtime interface and how to
update existing code.

## 1. Event handlers receive `&mut Model`

Update your event handlers to accept the model directly instead of an `EventContext`:

```rust
// Before
type UpdateFn = fn(Event, &mut EventContext<Event, Effect, Model>) -> Command<Event, Effect>;

fn update(event: Event, ctx: &mut EventContext<Event, Effect, Model>) -> Command<Event, Effect> {
    let model = ctx.model_mut();
    // ...
}

// After
type UpdateFn = fn(Event, &mut Model) -> Command<Event, Effect>;

fn update(event: Event, model: &mut Model) -> Command<Event, Effect> {
    // work with model directly
}
```

## 2. Effect handlers receive resources

Effects now get a cloned application resource value. Declare your dependencies once via
`builder.with_resources(...)` and capture what you need inside the returned `Task` closure.

```rust
#[derive(Clone)]
struct AppResources {
    http: Arc<HttpClient>,
    log_prefix: &'static str,
}

fn effects(effect: Effect, resources: AppResources) -> Task<Event, Effect> {
    match effect {
        Effect::Fetch(url) => Task::async_on::<TokioExecutor, _>(async move {
            let body = resources.http.get(&url).await?;
            Command::event(Event::Fetched(body))
        }),
        Effect::Log(msg) => Task::async_on::<InlineAsync<Event>, _>(async move {
            println!("{} {msg}", resources.log_prefix);
            Command::none()
        }),
    }
}

let mut app = Syzygy::builder::<Event, Effect>()
    .model(Model::default())
    .with_resources(AppResources {
        http: Arc::new(HttpClient::new()),
        log_prefix: "[app]",
    })
    .event_handler(update)
    .effect_handler(effects)
    .with_async_executor(TokioExecutor::multi_thread_io("io", 2))
    .build();
```

> **Note**: Resources are cloned for every effect invocation. Keep the type cheap to clone by
> wrapping heavy dependencies in `Arc` or other shared handles. Do not hand out `&mut` access;
> use interior mutability (e.g. `Arc<Mutex<T>>`) when necessary.

## 3. Tasks return `Command` values

Task closures now produce `Command<Event, Effect>` directly. The `Outcome` enum and
`EffectContext` have been removed.

```rust
// Before
Task::async_on::<TokioExecutor, _>(async move {
    runtime.do_work().await;
    Outcome::Event(Event::Done)
});

// After
Task::async_on::<TokioExecutor, _>(async move {
    do_work().await;
    Command::event(Event::Done)
});
```

Available helpers remain the same (`Task::none()`, `Task::events`, etc.), but they now build
commands under the hood.

## 4. Executors only manage runtime concerns

Executors no longer carry application resources:

- `InlineAsync::<Event>::new()` executes the job immediately and always passes `()` to the
  closure.
- `TokioExecutor` exposes `builder()`, `multi_thread_io(...)`, etc. without extra resource
  arguments. Use the global application resources (see §2) when tasks need shared state.
- Dedicated sync task helpers will return in a future release; for now, offload blocking work
  via async executors (for example using `tokio::task::spawn_blocking`).

To reuse an existing Tokio runtime, call `TokioExecutor::from_handle(handle)` or
`TokioExecutor::try_from_current()`.

## 5. Shell resources replace ad-hoc containers

If you previously used custom `Arc<Mutex<Option<T>>>` or bespoke dependency containers inside
`EffectContext`, move that state into the resource struct you pass to `with_resources`. This
keeps dependencies centralized and makes it obvious which services are available to effects.

---

With these changes in place you get:

- handlers with minimal ceremony,
- a single, well-defined home for app dependencies, and
- a unified `Command` pipeline across events and effects.
