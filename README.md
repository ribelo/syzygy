# Syzygy

Fast, type-safe state management library for Rust with effects and async support.

## What this library does

Syzygy provides a simple, composable way to manage application state with effects. It's built around three core concepts:

1. **State Management** - Store and update your application state safely
2. **Effect System** - Dispatch asynchronous operations without blocking
3. **Resource Storage** - Type-safe dependency injection for shared resources

## Quick Start

```rust
use syzygy::prelude::*;

// 1. Define your state model
#[derive(Debug, Clone)]
struct CounterModel {
    value: i32,
}

impl Model for CounterModel {
    type Snapshot = Self;
    fn to_snapshot(&self) -> Self::Snapshot {
        self.clone()
    }
}

// 2. Create the state container
let syzygy = Syzygy::builder()
    .model(CounterModel { value: 0 })
    .build();

// 3. Dispatch effects to modify state
syzygy.dispatch(|ctx| {
    ctx.update(|model| model.value += 1);
});

// 4. Access state safely
println!("Counter: {}", syzygy.model().value);
```

## Features

- **Zero-copy resource access** - ~15ns resource lookup via `Arc<T>`
- **Fast effect dispatch** - ~51ns per effect
- **Async support** - Built-in async task spawning with snapshots
- **Composable effects** - Chain timing, tracing, and debugging
- **Thread-safe** - Use across multiple threads safely
- **Type-safe** - Compile-time guarantees for state and resources

## Performance

Syzygy is designed for performance:

- Model reads: ~14ns
- Model updates: ~16ns  
- Resource access: ~15ns
- Effect dispatch: ~51ns
- Async task spawn: ~780ns

See `benchmark_results/` for detailed performance analysis.

## Why Syzygy?

- **Simple** - Three concepts: state, effects, resources
- **Fast** - Optimized for high-frequency operations
- **Safe** - No `Arc<Mutex<State>>` antipatterns
- **Composable** - Mix and match features as needed

## Installation

```toml
[dependencies]
syzygy = { git = "https://github.com/ribelo/syzygy" }
```

## Documentation

- [CLAUDE.md](CLAUDE.md) - Development guide and commands
- [Architecture](src/) - See source code for implementation details
- [Examples](tests/) - Check integration tests for usage patterns

## License

This project is licensed under the MIT License.