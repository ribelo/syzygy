# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Syzygy is an event-driven state management library written in Rust. It provides state management with context management, resource storage, and effect dispatching capabilities.

## Key Commands

### Build & Test
```bash
# Build the project
cargo build
cargo build --release

# Run all tests
cargo test

# Run a specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Lint the code
cargo clippy

# Format code
cargo fmt

# Check formatting without applying
cargo fmt -- --check
```

### Benchmarking
```bash
# Run all benchmarks and save results
./scripts/bench.sh

# Compare with baseline
./scripts/bench-compare.sh [baseline_name]

# Watch and re-run benchmarks on changes  
./scripts/bench-watch.sh

# Run benchmarks directly
cargo bench --bench syzygy_benchmarks

# Quick benchmark run
cargo bench --bench syzygy_benchmarks -- --warm-up-time 1 --measurement-time 2
```

## Architecture

### Core Components

1. **Syzygy** (`src/syzygy.rs`) - Main entry point providing the builder pattern API
2. **Model** (`src/model/`) - State storage with sync and unsync variants
3. **Resources** (`src/resource.rs`) - Type-erased resource storage system
4. **Dispatch** (`src/dispatch.rs`) - Effect dispatching and execution system
5. **Context** (`src/context/`) - Context management with async support

### Key Design Patterns

- **Builder Pattern**: `Syzygy::builder()` for configuration
- **Type Erasure**: Resources stored as `Any` types with downcasting
- **Event System**: Effects are typed events dispatched through channels
- **Async Support**: AsyncContext provides snapshot-based async operations

### Important Traits

- `Model` - Requires `Snapshot` associated type and `to_snapshot()` method
- `ModelAccess` / `ModelModify` - For accessing and modifying model state
- `ResourceAccess` / `ResourceModify` - For resource operations
  - `resource<T>()` returns `Arc<T>`
  - `resource_cloned<T>()` returns `T`
- `DispatchEffect` - For dispatching effects
- `FromContext` / `IntoContext` - Context conversion

## Development Guidelines

1. **Testing**: Use deterministic synchronization primitives, not `thread::sleep()`.

2. **Performance**: Benchmark all changes. See benchmarks in `benches/`.

3. **Linting**: Run `cargo clippy` before committing.

4. **Features**: 
   - `parallel` - Enables parallel execution using rayon

5. **Refactoring Roadmap**: See `todo/unfuck_roadmap.md` for planned improvements.

## Testing Individual Components

```bash
# Test specific module
cargo test model::
cargo test dispatch::
cargo test resource::

# Run benchmarks for specific component
cargo bench --bench syzygy_benchmarks -- model
cargo bench --bench syzygy_benchmarks -- dispatch
```

## Common Development Tasks

When working on effects system:
- Effects are typed events implementing the `Event<M>` trait
- Check `src/dispatch.rs` for the dispatch implementation
- Benchmarks in `benches/syzygy_benchmarks.rs` cover effect throughput

When working on resources:
- Resources stored as `Arc<T>` internally in `FxHashMap<TypeId, Box<dyn Any + Send + Sync>>`
- `resource<T>()` returns `Arc<T>`, `resource_cloned<T>()` returns `T`
- See `src/resource.rs` for implementation