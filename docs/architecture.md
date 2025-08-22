# Syzygy Architecture

## Executive Summary

Syzygy is The Elm Architecture (TEA) for Rust applications. It provides a clean, performant, and maintainable event-driven system focused on unidirectional data flow, pure functional updates, and clear separation between synchronous business logic and asynchronous effects.

## Core Philosophy

### Fundamental Principles

1. **Unidirectional Data Flow** - Events flow in one direction: Event → Model → Command → Effect → Event
2. **Pure Functional Core** - Business logic is deterministic and side-effect free  
3. **Error-as-Events** - All errors flow through the same event pipeline
4. **Effect-as-Data** - Side effects are described as data, not executed directly
5. **Safety First** - Memory safety and task cleanup are guaranteed by design
6. **Zero-Overhead Abstractions** - High performance through compile-time optimizations

## Architecture Overview

```
┌─────────────────────────────────────────────────────────┐
│                    External World                       │
│           (User Input, Network, Files, etc.)           │
└─────────────────┬───────────────────┬───────────────────┘
                  │ Events            │ Effects
                  ▼                   ▼
┌─────────────────────────────────────────────────────────┐
│               Core (Sync)     Shell (Async)            │
│                                                         │
│  ┌──────────────────────┐    ┌─────────────────────┐   │
│  │   Core               │    │   Shell             │   │  
│  │                      │    │                     │   │
│  │  - Owns Model        │    │  - EffectContext    │   │
│  │  - Processes Events  │───▶│  - Executes Effects │   │
│  │  - Returns Commands  │    │  - Routes Events    │   │
│  │  - Purely Sync      │    │  - Fully Async      │   │
│  └──────────────────────┘    └─────────────────────┘   │
│                                                         │
│               Command<Event, Effect>                    │
│          (Bridge between Core and Shell)               │
└─────────────────────────────────────────────────────────┘

Unidirectional Flow:
Event → Core.update() → Command → Shell.execute() → Effect → EffectContext → Event
```

## Core Components

### 1. App Trait - The Heart of Your Application

```rust
pub trait App: Send + 'static {
    type Event: Clone + Send + 'static;     // What can happen
    type Model: Send + 'static;             // Application state
    type Effect: Clone + Send + 'static;    // What you want to do
    
    fn update(
        &self, 
        event: Self::Event, 
        model: &mut Self::Model
    ) -> Command<Self::Event, Self::Effect>;
    
    #[cfg(feature = "view-model")]
    fn view(&self, model: &Self::Model) -> Self::ViewModel;
}
```

**The `update` function is the core of TEA**:
- **Pure function** - No I/O, no side effects, completely deterministic
- **Event → Model mutation** - Update state based on what happened
- **Return Command** - Describe what effects to run next
- **Testable** - Easy to test with just inputs and outputs

### 2. Events - What Can Happen

Events represent everything that can happen in your system:

```rust
#[derive(Debug, Clone)]
enum AppEvent {
    // User actions
    UserClicked { button: String },
    TextEntered { field: String, text: String },
    
    // External events  
    DataReceived { data: String },
    TimerExpired,
    
    // Error events (not exceptions!)
    ValidationFailed { field: String, reason: String },
    NetworkError { message: String },
    
    // Success events
    DataSaved { id: String },
    LoginSuccess { token: String },
}
```

**Key points**:
- **Errors are events** - No exceptions, no `Result` returns from `update()`
- **Clone-able** - For replay, debugging, and event sourcing
- **Self-describing** - Each event carries all needed data

### 3. Model - Your Application State

The single source of truth for all application state:

```rust
#[derive(Debug)]
struct AppModel {
    // Domain data
    users: Vec<User>,
    current_user: Option<User>,
    
    // UI state
    loading: bool,
    error_message: Option<String>,
    
    // Process state
    pending_operations: HashSet<String>,
}
```

**Design principles**:
- **Single source of truth** - No duplicate state anywhere
- **Owned by Core** - Only Core can mutate the model
- **Simple data structures** - Easy to serialize, debug, and reason about

### 4. Effects - What You Want To Do

Effects are data descriptions of side effects to perform:

```rust
#[derive(Debug, Clone)]
enum AppEffect {
    HttpGet { url: String },
    HttpPost { url: String, body: String },
    SaveFile { path: String, data: Vec<u8> },
    Sleep { duration: Duration },
    Log { level: LogLevel, message: String },
}
```

**Key insight**: Effects are **data, not functions**:
- **Testable** - You can assert effects were created without executing them
- **Serializable** - Can be logged, replayed, or sent over network
- **Runtime-agnostic** - Shell can execute with any async runtime

### 5. Commands - Simple Data Orchestration

Commands are simple data structures that describe what should happen:

```rust
impl<Event, Effect> Command<Event, Effect> {
    /// Create a no-op command
    pub fn none() -> Self
    
    /// Emit an event immediately  
    pub fn event(event: Event) -> Self
    
    /// Request an effect to be executed
    pub fn effect(effect: Effect) -> Self
    
    /// Emit multiple events
    pub fn events(events: impl IntoIterator<Item = Event>) -> Self
    
    /// Request multiple effects  
    pub fn effects(effects: impl IntoIterator<Item = Effect>) -> Self
    
    /// Combine multiple commands (effects run concurrently)
    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self
    
    /// Transform events/effects in a command
    pub fn map_event<F>(self, f: F) -> Command<NewEvent, Effect>
    pub fn map_effect<F>(self, f: F) -> Command<Event, NewEffect>
}
```

#### Execution Patterns

**Parallel Effects (Default)**: 
```rust
// Effects execute concurrently, events process sequentially
Command::batch([
    Command::effect(HttpGet { url: "/api/users".into() }),
    Command::effect(HttpGet { url: "/api/posts".into() }),
    Command::event(UIRefreshed), // Processes immediately
])
```

**Sequential Operations (Event-Chaining)**:
```rust
// Use events to enforce sequential execution
match event {
    StartWorkflow => Command::effect(Step1Effect),
    Step1Complete => Command::effect(Step2Effect),
    Step2Complete => Command::effect(Step3Effect),  
    WorkflowComplete => Command::none(),
}
```

**Mixed Patterns**:
```rust
Command::batch([
    Command::events([ClearUI, ShowLoading]),        // Immediate
    Command::parallel([SaveLocal, SaveRemote]),     // Parallel
    Command::event(LogActivity),                    // Immediate
])
```

**Usage in your `update` function**:

```rust
fn update(&self, event: AppEvent, model: &mut AppModel) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::LoginClicked => {
            if model.username.is_empty() {
                // Error as event
                Command::event(AppEvent::ValidationFailed {
                    field: "username".to_string(),
                    reason: "Username required".to_string(),
                })
            } else {
                // Request HTTP effect  
                Command::effect(AppEffect::HttpPost {
                    url: "/api/login".to_string(),
                    body: serde_json::to_string(&model.credentials).unwrap(),
                })
            }
        }
        
        AppEvent::ValidationFailed { field, reason } => {
            // Handle errors like any other event
            model.error_message = Some(format!("{}: {}", field, reason));
            Command::none()
        }
        
        AppEvent::DataReceived { data } => {
            // Parse and save
            if let Ok(user) = serde_json::from_str::<User>(&data) {
                model.current_user = Some(user);
                model.loading = false;
                Command::none()
            } else {
                Command::event(AppEvent::NetworkError {
                    message: "Invalid response format".to_string()
                })
            }
        }
    }
}
```

### 6. Core - Pure Event Processing Engine

```rust
impl<A: App> Core<A> {
    /// Process an event and return a command
    pub fn handle_event(&mut self, event: A::Event) -> Command<A::Event, A::Effect>
    
    /// Get current model (read-only)
    pub fn model(&self) -> &A::Model
    
    /// Send an event for processing  
    pub fn send_event(&self, event: A::Event) -> Result<(), CoreError>\n    \n    /// Get a sender for external events\n    pub fn event_sender(&self) -> Sender<A::Event>
    
    /// Process all pending events
    pub fn tick(&mut self) -> Vec<Command<A::Event, A::Effect>>
}
```

**Key characteristics**:
- **Synchronous** - Can run on any thread, including UI threads
- **No async/await** - Keeps business logic simple and testable  
- **Event-driven** - Only updates state in response to events
- **Command producer** - Returns descriptions of what to do, doesn't do it

### 7. Shell - Asynchronous Effect Execution

```rust
impl<A: App> Shell<A> {
    /// Set the effect handler that converts effects to async operations
    pub fn with_effect_handler<F>(self, handler: F) -> Self
    where F: Fn(A::Effect, EffectContext<A::Event>) -> BoxFuture<'static, ()>
    
    /// Execute a command synchronously, routing outputs to channels
    pub fn execute_command(&mut self, command: Command<A::Event, A::Effect>) -> Result<(), ShellError>
    
    /// Process effects and manage async tasks  
    pub fn tick(&mut self) -> Result<bool, ShellError>
}
```

**Effect handler pattern**:

```rust
fn handle_effects(effect: MyEffect, ctx: EffectContext<MyEvent>) -> BoxFuture<'static, ()> {
    Box::pin(async move {
        match effect {
            MyEffect::HttpGet { url } => {
                match reqwest::get(&url).await {
                    Ok(response) => {
                        let data = response.text().await.unwrap_or_default();
                        let _ = ctx.send_event(MyEvent::DataReceived { data });
                    }
                    Err(error) => {
                        let _ = ctx.send_event(MyEvent::NetworkError { 
                            message: error.to_string() 
                        });
                    }
                }
            }
            
            MyEffect::Sleep { duration } => {
                tokio::time::sleep(duration).await;
                let _ = ctx.send_event(MyEvent::TimerExpired);
            }
        }
    })
}
```

**EffectContext** provides safe task spawning:
- **Memory safety** - All spawned tasks cancelled when context drops
- **High performance** - 24x faster than previous implementations  
- **Event routing** - Easy way to send events back to Core

### 8. Builder Pattern - Simple System Construction

```rust
let (core, shell) = Syzygy::builder()
    .app(MyApp::default())
    .model(MyModel::default()) 
    .build();

let shell = shell.with_effect_handler(handle_effects);
```

**Two build modes**:
- **`build()`** - Auto-wired Shell connected to Core (recommended)
- **`build_manual()`** - Independent Core and Shell for advanced use cases

### 9. Runner - Application Orchestration

```rust
let mut runner = Runner::new(core, shell);

// Send initial events
runner.core().send_event(AppEvent::AppStarted)?;

// Run until some condition
runner.run_until(
    |core, _shell| core.model().should_quit,
    syzygy::spawn::spawner()  // Runtime-neutral spawning
).await?;
```

**Runner benefits**:
- **Simple orchestration** - Manages Core ↔ Shell communication
- **Runtime neutral** - Works with tokio, smol, async-std
- **Condition-based** - Run until model reaches desired state

## Unidirectional Architecture

### Why Unidirectional?

**Problem with bidirectional patterns**:
```rust
// WRONG: Bidirectional request/response creates complexity
let response = http_client.get("/api/data").await?;
process_response(response); // Where does this state go?

// WRONG: Hidden state mutations  
async fn complex_workflow() {
    let data = fetch_data().await?;
    let processed = process_data(data).await?; 
    save_result(processed).await?; // State scattered everywhere
}
```

**Syzygy's unidirectional solution**:
```rust
// RIGHT: Everything flows through events
AppEvent::FetchDataClicked => {
    Command::effect(AppEffect::HttpGet { url: "/api/data".to_string() })
}

// Effect execution sends result as event
AppEvent::DataReceived { data } => {
    model.data = parse_data(data)?;
    Command::effect(AppEffect::ProcessData { data: model.data.clone() })
}

// Processing result comes back as event
AppEvent::DataProcessed { result } => {
    model.processed_data = result;
    Command::effect(AppEffect::SaveData { data: model.processed_data.clone() })
}

// Save result comes back as event  
AppEvent::DataSaved => {
    model.status = "Complete".to_string();
    Command::none()
}
```

**Benefits achieved**:
1. **Predictable state** - All mutations happen in one place (Core)
2. **Debuggable** - Every state change is an event you can log/replay
3. **Testable** - Simulate any scenario by sending events
4. **Recoverable** - Errors become events, handled like any other event

### Error Handling - Errors as Events

**Traditional error handling problems**:
```rust
// WRONG: Exceptions break the flow
fn update(event: Event, model: &mut Model) -> Result<Command, Error> {
    match event {
        Event::ValidateInput => {
            if model.input.is_empty() {
                return Err(ValidationError::EmptyInput); // Breaks unidirectional flow!
            }
            // ...
        }
    }
}
```

**Syzygy's error-as-events solution**:
```rust  
// RIGHT: Errors are events
fn update(&self, event: Event, model: &mut Model) -> Command<Event, Effect> {
    match event {
        Event::ValidateInput => {
            if model.input.is_empty() {
                Command::event(Event::ValidationFailed {
                    field: "input".to_string(),
                    reason: "Input cannot be empty".to_string(),
                })
            } else {
                Command::effect(Effect::ProcessInput { data: model.input.clone() })
            }
        }
        
        Event::ValidationFailed { field, reason } => {
            model.error_message = Some(format!("{}: {}", field, reason));
            Command::none()
        }
        
        Event::NetworkError { message } => {
            model.network_status = NetworkStatus::Error(message);
            Command::effect(Effect::RetryAfter { seconds: 5 })
        }
    }
}
```

**Benefits**:
- **Unified handling** - All outcomes flow through the same pipeline
- **Recoverable** - Errors can trigger recovery actions
- **Debuggable** - Error events can be logged and replayed
- **Testable** - Easy to test error scenarios

## Testing Strategy

### Unit Testing - Pure Functions

Since `update()` is a pure function, testing is trivial:

```rust
use syzygy::prelude::*;

#[test]
fn test_user_login_validation() {
    let app = MyApp::default();
    let mut model = MyModel::default();
    
    // Test empty username
    let command = app.update(AppEvent::LoginClicked, &mut model);
    
    // Commands are simple data - inspect directly
    assert_eq!(command.len(), 1);
    let outputs: Vec<_> = command.into_iter().collect();
    assert!(matches!(
        outputs[0], 
        CommandOutput::Event(AppEvent::ValidationFailed { field, .. }) if field == "username"
    ));
}

#[test]
fn test_successful_data_processing() {
    let app = MyApp::default();
    let mut model = MyModel::default();
    
    // Simulate data received event
    let command = app.update(
        AppEvent::DataReceived { data: "valid data".to_string() },
        &mut model
    );
    
    // Check model was updated
    assert!(!model.data.is_empty());
    
    // Check next effect was requested
    let outputs: Vec<_> = command.into_iter().collect();
    assert_eq!(outputs.len(), 1);
    assert!(matches!(outputs[0], CommandOutput::Effect(AppEffect::ProcessData { .. })));
}
```

### Integration Testing - Event Flows

Test complete workflows by simulating event sequences:

```rust
#[tokio::test]
async fn test_complete_login_flow() {
    let (mut core, shell) = Syzygy::builder()
        .app(MyApp::default())
        .model(MyModel::default())
        .build();
        
    let shell = shell.with_effect_handler(mock_effect_handler);
    let mut runner = Runner::new(core, shell);
    
    // Start login flow
    runner.core().send_event(AppEvent::LoginClicked)?;
    
    // Process one tick
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Simulate successful response
    runner.core().send_event(AppEvent::DataReceived {
        data: r#"{"token": "abc123", "user": {"name": "Alice"}}"#.to_string()
    })?;
    
    runner.tick(syzygy::spawn::spawner()).await?;
    
    // Check final state
    let model = runner.core().model();
    assert!(model.current_user.is_some());
    assert_eq!(model.current_user.as_ref().unwrap().name, "Alice");
}
```

## Performance Characteristics

Syzygy achieves high performance through:

### Zero-Overhead Abstractions
- **Commands compile to data** - No runtime overhead for composition
- **Events are simple enums** - Minimal memory footprint
- **Pure functions** - Compiler can optimize aggressively

### High-Performance AsyncContext
- **24x faster task spawning** - Direct spawning without boxing overhead
- **Memory safety** - Automatic task cancellation prevents leaks
- **Batch operations** - Efficient handling of multiple tasks

### Predictable Performance
- **No hidden allocations** - All allocations are explicit
- **No garbage collection** - Rust's ownership model ensures deterministic cleanup
- **Minimal runtime overhead** - Simple event loop with no complex scheduling

**Benchmark targets**:
- Event processing: < 100ns
- Command creation: < 50ns  
- Task spawning: ~4ns
- Model access: ~7ns

## Deployment Patterns

### Web Applications
```rust
#[tokio::main]
async fn main() {
    let (core, shell) = Syzygy::builder()
        .app(WebApp::default())
        .model(WebModel::default())
        .build();
    
    let shell = shell.with_effect_handler(web_effects);
    let mut runner = Runner::new(core, shell);
    
    // Handle HTTP requests
    let app = Router::new()
        .route("/api/:action", post(handle_request))
        .with_state(Arc::new(Mutex::new(runner)));
        
    serve(app).await
}
```

### Desktop GUI (egui/iced)
```rust
// Core runs on UI thread (synchronous updates)
// Shell runs on background thread (async effects)
std::thread::spawn(move || {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        loop {
            shell.tick().unwrap();
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
    });
});
```

### CLI Applications  
```rust
#[tokio::main]
async fn main() {
    let (core, shell) = Syzygy::builder()
        .app(CliApp::default())
        .model(parse_args())
        .build();
        
    let mut runner = Runner::new(core, shell.with_effect_handler(cli_effects));
    
    runner.run_until(
        |core, _| core.model().finished,
        syzygy::spawn::auto_spawn
    ).await.unwrap();
    
    println!("Result: {:?}", runner.core().model().result);
}
```

## Alternatives Considered

### Why Not Request/Response?
We considered bidirectional request/response patterns but rejected them because:

1. **Complexity** - Callbacks, promises, and async/await in business logic
2. **Hidden State** - Response handling scatters state mutations
3. **Testing Difficulty** - Hard to test without mocking async operations
4. **Error Handling** - Exception-based error handling breaks unidirectional flow

### Why Not Complex Command Builders?
We considered rich builder APIs like `Http::get().then_send()` but chose simplicity:

1. **Learning Curve** - Simple data structures are easier to understand
2. **Flexibility** - Users can create their own builders if needed
3. **Debugging** - Plain data structures are easier to inspect
4. **Performance** - No runtime builder overhead

### Why Not Magic Dependency Injection?
We considered automatic model extraction but kept it optional:

1. **Explicitness** - Clear data flow is more important than convenience
2. **Performance** - No hidden cloning or complex extraction
3. **Simplicity** - Basic function signatures are easier to reason about

## Summary

Syzygy proves that The Elm Architecture principles, combined with Rust's ownership model, create an ideal foundation for maintainable, high-performance applications. 

**Key architectural decisions**:
1. **Unidirectional flow** - Events flow in one direction only
2. **Pure functional Core** - Business logic remains simple and testable  
3. **Error-as-events** - All outcomes flow through the same pipeline
4. **Effect-as-data** - Side effects described as data, not executed directly
5. **Core/Shell separation** - Clear boundary between pure and impure code
6. **Safety first** - Memory safety and resource cleanup guaranteed by design

**What makes Syzygy different**:
- **Simple and elegant** - No complex builders or magic abstractions
- **High performance** - 24x faster task spawning with safety guarantees
- **Truly unidirectional** - No hidden bidirectional communication
- **Runtime flexible** - Works with any async runtime
- **Easy to test** - Pure functions and event simulation

The result is an architecture that scales from simple CLI tools to complex web applications while maintaining the same clear, predictable patterns throughout.
