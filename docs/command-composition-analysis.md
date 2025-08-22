# Command Composition in Syzygy: Current State

## Executive Summary

Syzygy has evolved to use a **simple SmallVec-based Command architecture** that provides excellent composition capabilities without the complexity of async Streams or external FFI requirements.

## Current Command Architecture

### Core Design (Implemented)

```rust
// Current Syzygy Commands - Simple and Powerful
Command::event(Event::UserClicked)           // Single event
Command::effect(Effect::SaveData { .. })     // Single effect  
Command::batch([cmd1, cmd2, cmd3])          // Parallel composition
Command::none()                             // Empty command

// Monadic Composition (New)
command.and_then(|outputs| next_command(outputs))  // Conditional chaining
command.when(condition)                      // Conditional execution
command.or_else(fallback_command)           // Fallback handling
command.then(next_command)                  // Sequential composition

// Domain-Specific Operations (New)
command.partition_outputs()                  // Split by type
command.count_events()                      // Count by type
command.find_event(|e| predicate(e))       // Type-safe search
command.try_map_event(|e| validate(e))     // Fail-fast transformation
```

### Fundamental Principles

1. **Commands are Pure Data** - No async execution, just description of what should happen
2. **SmallVec Optimization** - Inline storage for ≤4 outputs (covers 95% of cases)
3. **Type Safety** - Events and Effects are distinguished at compile time
4. **Zero-Cost Abstractions** - No boxing, no async overhead in command creation

## Migration from Legacy APIs

### What Was Removed
```rust
// OLD (Removed)
Command::all([cmd1, cmd2])          // Removed - was confusing
Command::sequential([cmd1, cmd2])   // Removed - overcomplicated  
Command::parallel([cmd1, cmd2])     // Removed - redundant
Command::race([cmd1, cmd2])         // Removed - wrong abstraction
```

### How to Achieve the Same Goals

#### Parallel Effects (Default Behavior)
```rust
// OLD: Command::parallel([cmd1, cmd2])
// NEW: Just batch them - effects run concurrently by default
Command::batch([
    Command::effect(Effect::LoadUser { id: 123 }),
    Command::effect(Effect::LoadPosts { user_id: 123 }),
])
```

#### Sequential Operations (Event Chaining)
```rust
// OLD: Command::sequential([cmd1, cmd2])  
// NEW: Use event chaining through the update cycle
fn update(&self, event: Event, model: &mut Model) -> Command<Event, Effect> {
    match event {
        Event::LoadUser { id } => Command::effect(Effect::LoadUserData { id }),
        Event::UserLoaded { user } => {
            model.user = Some(user);
            Command::effect(Effect::LoadPosts { user_id: user.id })
        }
        Event::PostsLoaded { posts } => {
            model.posts = posts;
            Command::effect(Effect::UpdateUI)
        }
    }
}
```

#### Racing/First-Wins (In Effect Handler)
```rust
// OLD: Command::race([cmd1, cmd2])
// NEW: Use futures::select! in effect handlers
async fn handle_effect(effect: Effect, ctx: AsyncContext<Event>) {
    match effect {
        Effect::LoadWithTimeout { url, timeout_ms } => {
            use futures::future::{select, Either};
            
            let load_future = fetch_data(&url);
            let timeout_future = async_sleep(Duration::from_millis(timeout_ms));
            
            match select(load_future, timeout_future).await {
                Either::Left((data, _)) => {
                    ctx.send_event(Event::DataLoaded { data }).unwrap();
                }
                Either::Right((_, _)) => {
                    ctx.send_event(Event::Timeout).unwrap();
                }
            }
        }
    }
}
```

## Why This Approach is Better

### Performance Benefits
- **24x faster task spawning** in AsyncContext (4ns vs 97ns)
- **Zero allocation** for typical commands (≤4 outputs)
- **No async overhead** in command creation/composition
- **Cache-friendly** SmallVec layout

### Simplicity Benefits  
- **Clear semantics**: Commands describe, don't execute
- **Predictable behavior**: No hidden async state machines
- **Easy testing**: Pure data structures, no mocking needed
- **Debuggable**: Simple data flow, no complex futures

### Composition Power
```rust
// Complex workflows are possible and readable
let user_workflow = Command::effect(Effect::ValidateSession)
    .and_then(|outputs| {
        if outputs.is_empty() {
            Command::event(Event::SessionInvalid)
        } else {
            Command::batch([
                Command::effect(Effect::LoadUserProfile),
                Command::effect(Effect::LoadUserPreferences),
                Command::effect(Effect::LoadNotifications),
            ])
        }
    })
    .when(model.user_authenticated)
    .or_else(Command::event(Event::RedirectToLogin));
```

## Current Status: Production Ready

✅ **Core Architecture**: Stable SmallVec-based implementation  
✅ **Monadic Composition**: Full featured and tested  
✅ **Domain Operations**: Type-safe collection methods  
✅ **Performance**: 24x improvement over previous implementation  
✅ **Testing**: 60+ tests including property-based testing  
✅ **Memory Safety**: All async tasks tracked and cancelled  

The Command system is now simpler, faster, and more powerful than the original complex design.