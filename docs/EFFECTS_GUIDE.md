# The Definitive Guide to Effects

> **The One True Way**: There should be one—and preferably only one—obvious way to do it.

This guide establishes the canonical patterns for creating and dispatching effects in Syzygy. After reading this, you will understand the **one true path** to effect management.

## Philosophy

Syzygy's effect system follows a simple principle: **clarity over convenience**. Rather than offering multiple overlapping ways to accomplish the same task, we provide one clear, composable approach that scales from simple to complex use cases.

## The One True Path

### 1. Write Your Logic in a Closure

Effects are functions that take a mutable reference to your Syzygy context:

```rust
let my_effect = |ctx: &mut Syzygy<MyModel>| {
    ctx.update(|model| model.counter += 1);
};
```

### 2. Add Instrumentation (Optional)

If you need timing, tracing, or naming, use the `EffectExt` trait methods:

```rust
use syzygy::prelude::*;

// Timed effect
let timed_effect = (|ctx: &mut Syzygy<MyModel>| {
    ctx.update(|model| model.counter += 1);
}).timed("increment_counter");

// Traced effect
let traced_effect = (|ctx: &mut Syzygy<MyModel>| {
    ctx.update(|model| model.counter += 1);
}).traced();

// Named effect (for debugging)
let named_effect = (|ctx: &mut Syzygy<MyModel>| {
    ctx.update(|model| model.counter += 1);
}).named("increment_counter");

// Combined instrumentation
let instrumented_effect = (|ctx: &mut Syzygy<MyModel>| {
    ctx.update(|model| model.counter += 1);
}).timed("increment_counter").traced().named("debug_increment");
```

### 3. Build the Effect (If Using Instrumentation)

When you chain instrumentation methods, call `.build()` to create the final effect:

```rust
let final_effect = (|ctx: &mut Syzygy<MyModel>| {
    ctx.update(|model| model.counter += 1);
}).timed("increment_counter").build();
```

### 4. Dispatch the Effect

Pass your effect to `syzygy.dispatch()`:

```rust
// Simple effect
syzygy.dispatch(|ctx: &mut Syzygy<MyModel>| {
    ctx.update(|model| model.counter += 1);
});

// Instrumented effect
syzygy.dispatch(
    (|ctx: &mut Syzygy<MyModel>| {
        ctx.update(|model| model.counter += 1);
    }).timed("increment_counter").build()
);
```

## Common Patterns

### Model Updates

The most common pattern is updating your model:

```rust
// Direct update
syzygy.dispatch(|ctx| {
    ctx.update(|model| {
        model.counter += 1;
        model.last_updated = Instant::now();
    });
});
```

**Note**: We removed the `dispatch_update` convenience method. The explicit pattern above is clearer and teaches you the fundamental dispatch mechanism.

### Resource Access

Access resources within your effects:

```rust
syzygy.dispatch(|ctx| {
    let config = ctx.resource::<AppConfig>();
    ctx.update(|model| {
        model.max_value = config.max_allowed;
    });
});
```

### Async Operations

For async work, use `spawn` (for blocking operations) or `task` (for async operations):

```rust
// Blocking work
syzygy.spawn(|ctx| {
    let result = expensive_blocking_operation();
    ctx.dispatch(|inner_ctx| {
        inner_ctx.update(|model| model.result = result);
    });
});

// Async work
syzygy.task(|ctx| async move {
    let result = async_operation().await;
    ctx.dispatch(|inner_ctx| {
        inner_ctx.update(|model| model.result = result);
    });
});
```

### Error Handling

For effects that might fail, use `try_dispatch`:

```rust
match syzygy.try_dispatch(potentially_failing_effect) {
    Ok(()) => println!("Effect dispatched successfully"),
    Err(e) => eprintln!("Failed to dispatch effect: {}", e),
}
```

**Note**: Most applications should use `dispatch()`, which panics on channel failure. A closed effect channel indicates a fatal application error.

## Advanced: Async Dispatch Extensions

For async contexts, import `AsyncDispatchExt` to access `dispatch_sync`:

```rust
use syzygy::prelude::*;

async fn async_function(syzygy: &Syzygy<MyModel>) {
    let completion = syzygy.dispatch_sync(|ctx| {
        ctx.update(|model| model.counter += 1);
    });
    
    // Wait for the effect to complete
    completion.await.unwrap();
}
```

## Anti-Patterns

### ❌ DON'T: Use Multiple APIs

```rust
// Wrong - multiple ways to do the same thing
syzygy.dispatch_timed("my_effect", my_closure);  // REMOVED
syzygy.dispatch_traced(my_closure);              // REMOVED
syzygy.dispatch_builder(|| builder.build());    // REMOVED
```

### ❌ DON'T: Rely on Removed Conveniences

```rust
// Wrong - removed convenience methods
syzygy.dispatch_update(|model| model.counter += 1);  // REMOVED
syzygy.send_effect(my_effect);                        // REMOVED
```

### ✅ DO: Use the One True Way

```rust
// Correct - the canonical approach
syzygy.dispatch(
    (|ctx: &mut Syzygy<MyModel>| {
        ctx.update(|model| model.counter += 1);
    }).timed("increment").build()
);
```

## Migration Guide

If you're migrating from the old API:

| Old Code | New Code |
|----------|----------|
| `syzygy.dispatch_update(\|m\| ...)` | `syzygy.dispatch(\|ctx\| ctx.update(\|m\| ...))` |
| `syzygy.dispatch_timed("name", effect)` | `syzygy.dispatch(effect.timed("name").build())` |
| `syzygy.send_effect(effect)` | `syzygy.dispatch(effect)` |
| `effect!(\|ctx\| ...)` | `\|ctx\| ...` |
| `dispatch_sync` | Import `AsyncDispatchExt` |

## Summary

The path is simple:

1. **Write a closure** with your effect logic
2. **Chain instrumentation** if needed (`.timed()`, `.traced()`, `.named()`)
3. **Call `.build()`** if you used instrumentation
4. **Pass to `syzygy.dispatch()`**

This approach is:
- **Discoverable**: IntelliSense shows you the methods
- **Composable**: Chain multiple instrumentation types
- **Consistent**: Same pattern for all complexity levels
- **Clear**: No hidden magic or multiple ways to do things

Welcome to the simplified world of Syzygy effects. There is one way, and it is good.