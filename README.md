# Syzygy

A hybrid compile-time/runtime state management system for Rust following functional core/imperative shell architecture.

## Features

- 🚀 **Zero-overhead model access** via compile-time type chains
- 🔧 **Compile-time command routing** with type-safe handler chains
- 🌊 **Async effect system** for I/O operations
- 🎯 **Type safety** - accessing non-existent models won't compile
- 📦 **Small and fast** - uses SmallVec to avoid allocations

## Quick Start

```rust
use syzygy::prelude::*;

// Define your model
#[derive(Default, Clone)]
struct AppModel {
    counter: i32,
}

// Define commands
#[derive(Clone)]
struct Increment(i32);

// Define effects (for I/O)
#[derive(Clone)]
enum AppEffect {
    Log(String),
}

impl Effect<Increment> for AppEffect {
    async fn execute(&self, _ctx: EffectContext<Increment>) {
        match self {
            AppEffect::Log(msg) => println!("{}", msg),
        }
    }
}

// Build the system
let (mut syzygy, handle, _effects) = SimpleBuilder::new(AppModel::default())
    .command(|cmd: Increment| {
        vec![AppEffect::Log(format!("Incrementing by {}", cmd.0))].into()
    })
    .build();

// Send commands
handle.dispatch(Increment(5))?;

// Process them
syzygy.process_commands();
- **Thread-safe** - Use across multiple threads safely
- **Type-safe** - Compile-time guarantees for state and resources

## Performance

Syzygy is designed for performance:

- Model reads: ~7ns
- Model updates: ~7ns
- Resource access: ~15ns
- Effect dispatch: ~51ns
- Async task spawn: ~900ns

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