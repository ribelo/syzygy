# Async Support in Syzygy

This document explains Syzygy's async design, when to use it, and why it's designed the way it is.

## TL;DR

- Async tasks get **snapshots** of state, not live state
- This prevents data races and is **BY DESIGN**
- Use async for I/O, external APIs, and long-running operations
- Use sync effects for state mutations

## AsyncContext Design

### The Snapshot Approach

When you spawn an async task, it receives an `AsyncContext<M>` containing:

```rust
pub struct AsyncContext<M: Model> {
    model_snapshot: Arc<M::Snapshot>,     // Frozen state at task creation
    resources: Arc<Resources>,            // Shared resources
    effects_tx: EffectsTx<M>,            // Channel to dispatch effects back
}
```

**Key Point**: The `model_snapshot` is **frozen at the moment the task is created**. This is intentional.

### Why Snapshots?

```rust
// WRONG: This would be a data race nightmare
// struct AsyncContext<M> {
//     model: Arc<Mutex<M>>,  // ❌ DON'T DO THIS
// }

// CORRECT: Snapshots prevent races
struct AsyncContext<M: Model> {
    model_snapshot: Arc<M::Snapshot>,  // ✅ Safe, immutable
}
```

The snapshot approach:
- **Prevents data races** - No shared mutable state
- **Avoids deadlocks** - No mutex contention
- **Provides consistency** - Task sees consistent state view
- **Enables parallelism** - Multiple tasks can run without blocking

### Making Snapshots Efficient

For efficient snapshots, use `Arc` internally in your models:

```rust
// ✅ GOOD: Cheap to snapshot
#[derive(Debug, Clone)]
struct EfficientModel {
    data: Arc<LargeData>,
    counter: i32,
}

impl Model for EfficientModel {
    type Snapshot = Self;
    fn to_snapshot(&self) -> Self::Snapshot {
        self.clone()  // Only clones the Arc, not the data
    }
}

// ❌ BAD: Expensive to snapshot
#[derive(Debug, Clone)]
struct ExpensiveModel {
    large_vec: Vec<BigStruct>,  // Clones entire vector
}
```

## Usage Patterns

### Sync vs Async Decision Tree

```
Need to modify state immediately?
├─ Yes → Use sync effects
│   syzygy.dispatch(|ctx| {
│       ctx.update(|model| model.counter += 1);
│   });
│
└─ No → Need to do I/O or long computation?
    ├─ Yes → Use async tasks
    │   syzygy.task(|ctx| async move {
    │       let data = fetch_from_api().await;
    │       ctx.dispatch(|ctx| {
    │           ctx.update(|model| model.data = data);
    │       });
    │   });
    │
    └─ No → Use sync effects
```

### Async Patterns

#### 1. API Calls

```rust
// Fetch data and update state
syzygy.task(|ctx| async move {
    match fetch_user_data(ctx.model_snapshot.user_id).await {
        Ok(data) => {
            ctx.dispatch(|ctx| {
                ctx.update(|model| model.user_data = Some(data));
            });
        }
        Err(e) => {
            ctx.dispatch(|ctx| {
                ctx.update(|model| model.error = Some(e));
            });
        }
    }
});
```

#### 2. Background Processing

```rust
// Long-running computation
syzygy.spawn(|ctx| {
    let input = ctx.model_snapshot.input_data.clone();
    
    // Heavy computation with snapshot data
    let result = expensive_computation(input);
    
    // Send result back via effect
    ctx.dispatch(move |ctx| {
        ctx.update(|model| model.result = Some(result));
    });
});
```

#### 3. Periodic Tasks

```rust
// Background cleanup every 30 seconds
syzygy.task(|ctx| async move {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        
        // Use current snapshot for cleanup decision
        let should_cleanup = ctx.model_snapshot.last_cleanup 
            + Duration::from_secs(300) < Instant::now();
            
        if should_cleanup {
            cleanup_resources().await;
            ctx.dispatch(|ctx| {
                ctx.update(|model| model.last_cleanup = Instant::now());
            });
        }
    }
});
```

## Common Pitfalls

### ❌ Expecting Live State in Async

```rust
// WRONG: Expecting fresh state
syzygy.task(|ctx| async move {
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // This is still the OLD snapshot from 5 seconds ago!
    let stale_value = ctx.model_snapshot.counter;
    
    ctx.dispatch(move |ctx| {
        // This will use stale data
        ctx.update(|model| model.result = stale_value * 2);
    });
});
```

### ✅ Working with Snapshots

```rust
// CORRECT: Use snapshot for input, dispatch for output
syzygy.task(|ctx| async move {
    let input = ctx.model_snapshot.input_value;
    
    let result = process_async(input).await;
    
    ctx.dispatch(move |ctx| {
        // Always use fresh state in effects
        ctx.update(|model| {
            model.processed_results.push(result);
        });
    });
});
```

## Performance Considerations

### Snapshot Creation Cost

- Make `to_snapshot()` as cheap as possible
- Use `Arc<T>` for large data structures
- Consider using `Cow<'_, T>` for copy-on-write semantics

### Async Task Overhead

Current benchmarks show:
- Async task spawn: ~780ns
- This is the cost of creating the snapshot + tokio overhead
- Very reasonable for I/O-bound operations

### Memory Usage

- Each async task holds a snapshot
- Snapshots share data via `Arc` when designed properly
- No significant memory overhead with good model design

## Feature Flags

Async support is optional:

```toml
[dependencies]
syzygy = { git = "...", default-features = false }  # Sync only
syzygy = { git = "...", features = ["async"] }      # With async
```

When async is disabled:
- `AsyncContext` doesn't exist
- `task()` and `spawn()` methods aren't available
- Smaller binary size
- No tokio dependency

## Design Philosophy

The async design follows these principles:

1. **Explicit over implicit** - Clear when you're working with snapshots
2. **Safety over convenience** - No data races, even if it means extra effects
3. **Performance by design** - Snapshots enable parallelism without locks
4. **Composable** - Async tasks can dispatch sync effects

This design has been battle-tested in production and provides both safety and performance for real-world applications.