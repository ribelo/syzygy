# Runner Event Processing Cycle

## Overview

The `Runner` in Syzygy orchestrates the interaction between `Core` (pure event processing) and `Shell` (async effect execution). Understanding the event processing cycle is crucial for building reliable applications and debugging event flow.

## Event Processing Flow

### Single Step Execution

Each call to `runner.tick()` follows this precise sequence:

```
1. Poll External Events (if no queued events)
   └── runner.core.poll_external_events()

2. Process Events to Stability
   ├── Loop until no more events:
   │   ├── process_queued_events() → Commands
   │   ├── route_command() for each Command
   │   │   ├── Route Events → Core event queue
   │   │   └── Route Effects → Shell effect queue
   │   └── Poll External Events (if no queued events)
   
3. Process Effects
   └── shell.tick() → Execute pending effects asynchronously

4. Return: did_work = true if any work was done
```

**Note**: The diagram shows `route_command()` as the core routing function. In the public API, 
`Shell::dispatch()` wraps `route_command()` and is the main interface for 
routing commands to their appropriate channels.

### Key Properties

1. **Event Processing to Stability**: Core processes ALL queued events before Shell execution
2. **Effect-Generated Events**: Events from effects are processed in the NEXT step cycle
3. **External Event Polling**: Only occurs when no events are queued (prevents starvation)
4. **Sequential Effect Processing**: Effects execute after event stability is reached

## Detailed Step Breakdown

### Step 1: External Event Polling

```rust
// Only poll if no queued work exists
if !self.core.has_queued_events() {
    self.core.poll_external_events();
}
```

**Purpose**: Bring in new events from external sources (channels, UI, timers, etc.)
**Condition**: Only when event queue is empty to prevent external events from starving internal event processing.

### Step 2: Event Processing Loop

```rust
loop {
    // Process events → commands
    let commands = self.core.process_queued_events();
    if commands.is_empty() {
        break; // Stability reached
    }
    
    // Execute commands (route events/effects)
    for command in commands {
        self.shell.dispatch(command)?;
    }
    
    // Poll external events between batches (if queue empty)
    if !self.core.has_queued_events() {
        self.core.poll_external_events();
    }
}
```

**Stability Condition**: No more events in queue AND no commands generated
**Command Execution**: Routes events back to Core, effects to Shell
**Interleaved Polling**: External events can enter between processing batches

### Step 3: Effect Processing

```rust
let shell_work = self.shell.tick(spawn_fn)?;
```

**Purpose**: Execute async effects that were queued during event processing
**Timing**: Only after event stability is reached
**Concurrency**: Effects execute asynchronously, may generate events for next cycle

## Event Flow Diagram

```
External Events → Core Event Queue
                     │
                     ▼
              ┌─────────────┐
              │   Events    │
              │ Processing  │
              │    Loop     │
              └─────────────┘
                     │
                     ▼
┌─────────────────────────────────────┐
│           Commands                  │
├─────────────────┬───────────────────┤
│     Events      │      Effects      │
│       │         │         │         │
│       ▼         │         ▼         │
│ Core Event      │   Shell Effect    │
│    Queue        │      Queue        │
│ (next cycle)    │   (async exec)    │
└─────────────────┴───────────────────┘
                     │
                     ▼
               Effect Results
              (events for next
                  cycle)
```

## Important Timing Considerations

### Effect-Generated Events Are Delayed

When an effect sends an event back to Core, it doesn't get processed immediately:

```rust
// In effect handler
async fn handle_effect(effect: MyEffect, ctx: EffectContext<MyEvent>) {
    match effect {
        MyEffect::FetchData => {
            let data = fetch_from_api().await;
            // This event goes to Core's event queue
            ctx.send_event(MyEvent::DataFetched { data })?;
            // ↑ Will be processed in NEXT runner.tick() call
        }
    }
}
```

**Implication**: Effects cannot immediately influence the current event processing cycle.

### Event Processing is Synchronous

All event processing within a single step is synchronous and deterministic:

```rust
// This sequence is predictable and immediate
runner.core().send_event(Event::Start)?;
// Event::Start gets processed in this step
// Any generated commands execute immediately
// Generated events queue for next step
```

### Fairness vs. Responsiveness

The Runner balances fairness (effects get CPU time) with responsiveness (events processed promptly):

- **High-frequency events**: May dominate CPU if continuously generated
- **Long-running effects**: Don't block event processing
- **Effect backpressure**: Shell queues effects but doesn't block Core

## Common Patterns and Gotchas

### Pattern: Event-Effect-Event Chain

```rust
// Step 1: User action
Event::ButtonClicked 
    → Effect::FetchData

// Step 2: Effect completes  
Effect::FetchData completes
    → Event::DataFetched

// Step 3: Update UI
Event::DataFetched
    → Effect::UpdateUI
```

Each arrow represents a step boundary - effects cannot complete within the same step.

### Pattern: Event Cascades

```rust
Event::UserLogin
    → Event::ValidateCredentials  // Same step
        → Event::CredentialsValid  // Same step  
            → Effect::LoadUserData // Queued for effect processing
                → Event::UserDataLoaded // Next step
```

Multiple events can cascade within a single step, but effects always introduce a step boundary.

### Gotcha: Infinite Event Loops

```rust
// BAD: Creates infinite loop
fn update(event: Event, model: &mut Model) -> Command<Event, Effect> {
    match event {
        Event::Ping => Command::event(Event::Ping), // ← Infinite!
    }
}
```

The Runner will detect runaway event processing and may apply limits or warnings.

### Gotcha: Effect Ordering

```rust
// These effects may execute in any order:
Command::all([
    Effect::SaveToDatabase,
    Effect::SendEmail,
    Effect::LogActivity,
])
```

Effects execute concurrently. Use sequential commands if ordering matters:

```rust
Command::sequential([
    Command::effect(Effect::SaveToDatabase),
    Command::effect(Effect::SendEmail),
    Command::effect(Effect::LogActivity),
])
```

## Configuration Options

### Runner Configuration

```rust
let runner = Runner::with_config(
    core,
    shell,
    RunnerConfig {
        max_run_duration: Some(Duration::from_secs(30)),
        idle_sleep: Duration::from_millis(10),
        debug_logging: true,
    }
);
```

- **max_run_duration**: Timeout for `run_until()` operations
- **idle_sleep**: Sleep duration when no work is available
- **debug_logging**: Enable step-by-step logging

### Shell Configuration

```rust
let shell = Shell::new()
    .with_config(ShellConfig {
        effect_timeout: Some(Duration::from_secs(30)),
        runtime: Time::Tokio,
    });
```

- **effect_timeout**: Individual effect timeout
- **runtime**: Async runtime implementation

## Debugging Event Flow

### Enable Debug Logging

```rust
let runner = Runner::with_config(
    core,
    shell,
    RunnerConfig {
        debug_logging: true,
        ..Default::default()
    }
);
```

### Manual Step Execution

```rust
// Single tick for precise control
let did_work = runner.tick(spawn_fn).await?;
println!("Step completed, work done: {}", did_work);
println!("Model state: {:?}", runner.core().model());
```

### Event Tracing

Enable the `tracing` feature for detailed event flow:

```toml
[dependencies]
syzygy = { version = "0.1", features = ["tracing"] }
```

## Example: Complete Event Cycle

```rust
use syzygy::prelude::*;
use std::time::Duration;

#[derive(Debug, Default)]
struct Model {
    step: String,
    data: Option<String>,
}

#[derive(Debug, Clone)]
enum Event {
    Start,
    BeginFetch,
    DataFetched { data: String },
    Complete,
}

#[derive(Debug, Clone)]
enum Effect {
    FetchData,
    LogCompletion,
}

struct MyApp;

impl App for MyApp {
    type Event = Event;
    type Model = Model;
    type Effect = Effect;
    
    fn update(&self, event: Event, model: &mut Model) -> Command<Event, Effect> {
        match event {
            Event::Start => {
                model.step = "Starting".to_string();
                Command::event(Event::BeginFetch) // Same step
            }
            Event::BeginFetch => {
                model.step = "Fetching".to_string();
                Command::effect(Effect::FetchData) // Next step
            }
            Event::DataFetched { data } => {
                model.step = "Processing".to_string();
                model.data = Some(data);
                Command::event(Event::Complete) // Same step
            }
            Event::Complete => {
                model.step = "Done".to_string();
                Command::effect(Effect::LogCompletion) // Next step
            }
        }
    }
}

async fn handle_effect(effect: Effect, ctx: EffectContext<Event>) {
    match effect {
        Effect::FetchData => {
            // Simulate async work
            tokio::time::sleep(Duration::from_millis(100)).await;
            let _ = ctx.send_event(Event::DataFetched {
                data: "API Response".to_string()
            });
            // ↑ This event will be processed in the NEXT step
        }
        Effect::LogCompletion => {
            println!("Process completed!");
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (core, shell) = Syzygy::builder::<MyApp>()
        .app(MyApp)
        .model(Model::default())
        .build();
    
    let event_sender = core.event_sender();
    let shell = shell.with_effect_handler(handle_effect);
    let mut runner = Runner::with_config(
        core,
        shell,
        RunnerConfig {
            debug_logging: true,
            ..Default::default()
        }
    );
    
    // Step 1: Start event cascade
    event_sender.send(Event::Start)?;
    
    println!("=== Step 1 ===");
    let _ = runner.tick(syzygy::spawn::TokioSpawn).await?;
    println!("Model: {:?}", runner.core().model());
    // Output: Model { step: "Fetching", data: None }
    // Events processed: Start → BeginFetch → Effect::FetchData queued
    
    println!("=== Step 2 ===");
    let _ = runner.tick(syzygy::spawn::TokioSpawn).await?;
    println!("Model: {:?}", runner.core().model());
    // Output: Model { step: "Done", data: Some("API Response") }
    // Effect::FetchData executed → Event::DataFetched → Event::Complete → Effect::LogCompletion queued
    
    println!("=== Step 3 ===");
    let _ = runner.tick(syzygy::spawn::TokioSpawn).await?;
    println!("Model: {:?}", runner.core().model());
    // Output: "Process completed!" logged
    // Effect::LogCompletion executed
    
    Ok(())
}
```

## Summary

The Runner's event processing cycle ensures:

1. **Deterministic event processing**: Events are processed to stability before effects
2. **Non-blocking effects**: Async effects don't block event processing  
3. **Fair execution**: Both events and effects get CPU time
4. **Predictable timing**: Effect-generated events always appear in the next cycle

Understanding this cycle is essential for building reliable Syzygy applications and reasoning about event timing and flow.
