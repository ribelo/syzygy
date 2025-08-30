# Streaming Effects API Proposal for Syzygy (Revised)

*This document is a revision of the original proposal. The key change, based on feedback, is the unification of how single-event and streaming-event effects are handled by the coordination API. This creates a more consistent, flexible, and powerful system.*

## Requirements

### Core Problem
Syzygy needs to handle long-running operations that produce multiple events over time:
- File downloads with progress updates
- File system watching
- Server-sent events  
- Database change streams
- Background job progress

### Constraints
- **Effects must remain data** - no `Command::perform(async {})`
- **Maintain error-as-events** - all failures flow through the event pipeline
- **Preserve streaming behavior** - no collecting streams into vectors
- **Automatic resource cleanup** - no manual stream lifecycle management

## Proposed API

### Effect Handler Return Types

The foundation of the API is the `EffectOutput` enum, which allows effect handlers to return either a single event or a stream of events.

```rust
// Effect handlers can return single events or event streams
pub enum EffectOutput<Event> {
    Single(Event),                           // A single, discrete event
    Stream(BoxStream<'static, Event>),       // A stream of events over time
    None,                                    // Fire-and-forget, no event produced
}

pub type EffectResult<Event> = Result<EffectOutput<Event>, EffectError>;
```

The runtime transparently treats `EffectOutput::Single(event)` as a stream containing one event that completes immediately. This is the key principle that enables a unified coordination API.

### Effect Handler Examples

These examples remain unchanged, as they correctly demonstrate returning single vs. streaming outputs.

```rust
// Single-value effect
async fn handle_load_config(...) -> EffectResult<AppEvent> {
    let config = fs.read_config("app.toml").await?;
    Ok(EffectOutput::Single(AppEvent::ConfigLoaded { config }))
}

// Streaming effect with progress
async fn handle_download_file(
    DownloadFileEffect { url, destination }: DownloadFileEffect,
    http_client: &HttpClient,
) -> EffectResult<AppEvent> {
    let progress_stream = async_stream::stream! {
        let mut response = http_client.get(&url).send().await?;
        let total = response.content_length().unwrap_or(0);
        let mut downloaded = 0u64;
        let mut file = tokio::fs::File::create(&destination).await?;

        while let Some(chunk) = response.chunk().await? {
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            yield AppEvent::DownloadProgress { url: url.clone(), downloaded, total };
        }

        yield AppEvent::DownloadComplete { url, path: destination };
    };
    
    Ok(EffectOutput::Stream(progress_stream.boxed()))
}

// Infinite streaming effect  
async fn handle_watch_directory(
    WatchDirectoryEffect { path }: WatchDirectoryEffect,
    notify_service: &NotifyService,
) -> EffectResult<AppEvent> {
    let watcher_stream = async_stream::stream! {
        let mut watcher = notify_service.watch(&path)?;
        
        while let Some(event) = watcher.next().await {
            match event {
                Ok(file_event) => yield AppEvent::FileChanged { 
                    path: path.clone(), 
                    event: file_event 
                },
                Err(e) => yield AppEvent::WatchError { 
                    path: path.clone(), 
                    error: e.to_string() 
                }
            }
        }
    };
        
    Ok(EffectOutput::Stream(watcher_stream.boxed()))
}
```

### Unified Coordination API

With all effects being treated as streams, the artificial distinction between "Future coordination" and "Stream coordination" is eliminated. We now have one set of powerful, composable primitives.

```rust
// Unified coordination data structures
#[derive(Debug)]
pub enum CommandStep<Event, Effect> {
    Event(Event),
    Effect(Effect),
    Batch(Vec<Effect>),
    
    // Unified coordination primitives that work for ALL effects
    Merge {
        effects: Vec<Effect>,
        barrier_event: Option<Event>, // Dispatched when all merged streams complete
    },
    Chain {
        effects: Vec<Effect>,
        barrier_event: Option<Event>, // Dispatched when the final chained stream completes
    },
    Race {
        effects: Vec<Effect>,
        timeout_per: Option<Duration>,
        barrier_event: Option<Event>, // Dispatched when a winner is selected
    },
    Join { // Convenience wrapper for a common Merge pattern
        effects: Vec<Effect>,
        timeout_per: Option<Duration>,
        barrier_event: Option<Event>, // Dispatched when all joined streams complete
    },
    TryJoin {
        effects: Vec<Effect>,
        timeout_per: Option<Duration>, 
        barrier_event: Option<Event>, // Dispatched only if all succeed
    },
}

// Monadic composition API
impl<Event, Effect> Command<Event, Effect> {
    // Unified coordination - works on all effects
    pub fn merge(effects: impl IntoIterator<Item = Effect>) -> Self { /* ... */ }
    pub fn chain(effects: impl IntoIterator<Item = Effect>) -> Self { /* ... */ }
    pub fn race(effects: impl IntoIterator<Item = Effect>) -> Self { /* ... */ }
    
    // Convenience APIs for common patterns
    pub fn join(effects: impl IntoIterator<Item = Effect>) -> Self { /* ... */ }
    pub fn try_join(effects: impl IntoIterator<Item = Effect>) -> Self { /* ... */ }
    
    // Modifiers
    pub fn timeout_per(self, duration: Duration) -> Self { /* ... */ }
    pub fn barrier_event(self, event: Event) -> Self { /* ... */ }
}
```

### Unified Coordination Patterns

All coordination patterns operate on `Effect`s. The runtime handles whether the `EffectOutput` is `Single` or `Stream` by treating a single event as a stream of one item.

**`Merge`**: The fundamental parallel operator. Executes effects in parallel and merges their event streams. Events are dispatched to `update` as they arrive from any effect.
*Use for*: Combining multiple streaming sources like file watchers, WebSocket connections, and one-off setup tasks into a single event flow.

**`Join`**: A common pattern for parallel execution, best thought of as a `Merge` that is primarily concerned with the completion of all tasks. It executes effects in parallel, allows their individual events to flow to `update`, and dispatches a `barrier_event` only when all effects have completed their streams.
*Use for*: Bootstrap operations where all components must be ready before proceeding, but you still want to react to their individual completion events (e.g., `ConfigLoaded`, `UserLoaded`).

**`Race`**: Executes effects in parallel. The first effect to produce an event "wins". The event stream from the winning effect is selected to continue, and all other effects in the race are cancelled.
*Use for*: Failover scenarios, such as trying multiple API mirrors and picking the first to respond.

**`Chain`**: The fundamental sequential operator. Executes effects one after another. The event stream from the first effect must complete entirely before the next effect in the chain begins.
*Use for*: Ordered workflows where one step depends on the completion of the previous one, like a multi-stage data processing pipeline.

**`TryJoin`**: A variant of `Join` with error short-circuiting. It executes effects in parallel, but if any effect produces a designated "failure" event, all other running effects in the group are immediately cancelled. The `barrier_event` is only dispatched if all effects complete without any of them producing a failure event.
*Use for*: Critical, all-or-nothing operations where any single failure should halt the entire process. This pattern requires a clear way to identify failure events (e.g., an `is_failure()` method on the event enum).

### Usage Examples

The unified API is more expressive and flexible.

```rust
fn update_app(event: AppEvent, ctx: &mut EventContext<AppEvent, MyEffect, Storage>) -> Command<AppEvent, MyEffect> {
    match event {
        // Join: wait for all bootstrap effects to complete.
        // This works regardless of whether they return a single event or a stream.
        AppEvent::StartBootstrap => {
            Command::join([
                MyEffect::LoadConfig,     // -> Single(ConfigLoaded)
                MyEffect::LoadUser,       // -> Single(UserLoaded)  
                MyEffect::ConnectDatabase // -> Single(DatabaseConnected)
            ])
            .timeout_per(Duration::from_secs(10))
            .barrier_event(AppEvent::BootstrapComplete) // Emitted when all 3 are done
        }
        
        // Race: first effect to produce an event wins, others are cancelled.
        AppEvent::SelectFastestMirror => {
            Command::race([
                MyEffect::TryMirror { url: "mirror1.com".to_string() },
                MyEffect::TryMirror { url: "mirror2.com".to_string() }, 
                MyEffect::TryMirror { url: "mirror3.com".to_string() }
            ])
            .timeout_per(Duration::from_secs(5))
        }

        // Merge: combine multiple event streams.
        // Here we seamlessly mix a single-event effect with a streaming one.
        AppEvent::StartDashboard => {
            Command::merge([
                // This effect produces a single `InitialDataLoaded` event
                MyEffect::LoadInitialData,
                // This effect produces a long-running stream of `DashboardUpdate` events
                MyEffect::SubscribeToUpdates,
            ])
        }
        
        // Chain: sequential stream processing.
        AppEvent::StartBatchProcessing { batches } => {
            let batch_effects = batches.into_iter()
                .map(|batch| MyEffect::ProcessBatch { items: batch }) // Each -> Stream<Progress> + Single<Complete>
                .collect::<Vec<_>>();
                
            Command::chain(batch_effects)  
                .barrier_event(AppEvent::AllBatchesComplete) // Emitted when the last batch is done
        }
        
        // Handle events as they arrive
        AppEvent::ConfigLoaded { .. } | AppEvent::UserLoaded { .. } => {
            // Individual bootstrap events can be handled here
            Command::none()
        },
        AppEvent::BootstrapComplete => {
            println!("System is ready!");
            Command::none()
        }
    }
}
```

## Stream Lifecycle and Error Handling

The principles of stream lifecycle (natural termination, infinite streams, automatic cleanup via `Drop`) and error handling (errors-as-events) from the original proposal remain the same, as they are compatible with this unified coordination model. All errors from effect handlers should be mapped to specific error events that are returned via the `Ok(EffectOutput::Single(AppEvent::SomeError))` variant, ensuring they are processed on the main event loop.