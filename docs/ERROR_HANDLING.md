# Error Handling in Syzygy

This document describes Syzygy's approach to error handling, the types of errors you'll encounter, and how to handle them effectively.

## Philosophy

Syzygy follows a **function-specific error design**. Rather than using a single large error enum, each function defines its own error type that represents only the failures that specific function can produce. This approach provides several benefits:

- **Explicitness**: Each function's error signature tells you exactly what can go wrong
- **Composability**: Errors can be easily combined using `?` operator and `Box<dyn Error>`
- **Type Safety**: No need to handle irrelevant error variants
- **Maintainability**: Adding new operations doesn't require updating a central error enum

All error types implement the standard `std::error::Error` trait using the [`thiserror`](https://docs.rs/thiserror/) crate.

## Core Error Types

### `ResourceNotFoundError`

**Purpose**: Returned when attempting to access a resource that hasn't been registered.

**Common Scenarios**:
- Calling `get_resource::<T>()` or `get_resource_cloned::<T>()` for an unregistered type
- Using `expect_resource::<T>()` when the resource doesn't exist

```rust
use syzygy::prelude::*;

let syzygy = Syzygy::builder().model(MyModel::default()).build();

// This will return ResourceNotFoundError since Config wasn't registered
match syzygy.get_resource::<Config>() {
    Ok(config) => println!("Config: {:?}", config),
    Err(e) => {
        // e.type_name contains "Config"
        // e.type_id contains TypeId::of::<Config>()
        eprintln!("Resource error: {}", e);
    }
}
```

### `DispatchError`

**Purpose**: Returned when effect dispatch operations fail.

**Common Scenarios**:
- Channel is closed or full when dispatching effects
- Effect processing pipeline encounters issues

```rust
use syzygy::prelude::*;

// This error might occur in advanced scenarios where dispatch fails
match syzygy.dispatch_safe(my_effect) {
    Ok(()) => println!("Effect dispatched successfully"),
    Err(e) => eprintln!("Dispatch failed: {}", e.reason),
}
```

### `ContextCreationError`

**Purpose**: Returned when async context creation fails.

**Common Scenarios**:
- Model snapshot creation fails
- Context initialization encounters issues

```rust
#[cfg(feature = "async")]
use syzygy::prelude::*;

// This might fail if model snapshotting fails
match AsyncContext::create(syzygy).await {
    Ok(ctx) => println!("Async context created"),
    Err(e) => eprintln!("Context creation failed: {}", e.reason),
}
```

### `LockAcquisitionError`

**Purpose**: Returned when internal lock acquisition fails.

**Common Scenarios**:
- Lock poisoning due to panic in another thread
- Deadlock detection (in advanced implementations)

**Note**: Most Syzygy APIs handle locks internally, so you'll rarely see this error directly.

### `ResourceReplaceError`

**Purpose**: Returned when resource replacement operations fail.

**Common Scenarios**:
- Type mismatches in resource replacement
- Resource mutation conflicts

## Error Handling Patterns

### 1. Option vs. Result APIs

Syzygy provides both fallible and non-fallible resource access:

```rust
use syzygy::prelude::*;

// Option API - returns None for missing resources
if let Some(config) = syzygy.try_resource::<Config>() {
    println!("Found config: {:?}", config);
} else {
    println!("Config not found, using defaults");
}

// Result API - returns specific error for missing resources  
match syzygy.get_resource::<Config>() {
    Ok(config) => println!("Found config: {:?}", config),
    Err(e) => {
        eprintln!("Resource error: {}", e);
        // Handle error specifically, maybe log details
    }
}

// Panic API - panics if resource is missing (use sparingly)
let config = syzygy.resource::<Config>(); // Panics if not found
```

**Choose the right API**:
- Use `try_resource()` when missing resources are expected and you have fallback logic
- Use `get_resource()` when you need specific error information
- Use `resource()` only when the resource is guaranteed to exist

### 2. Error Composition

Syzygy errors compose well with standard Rust error handling:

```rust
use syzygy::prelude::*;

// Function that might fail for multiple reasons
fn process_request(syzygy: &Syzygy<MyModel>) -> Result<String, Box<dyn std::error::Error>> {
    let config = syzygy.get_resource_cloned::<Config>()?;
    let database = syzygy.get_resource_cloned::<Database>()?;
    
    // Use both resources...
    let result = database.query(&config.endpoint)?;
    Ok(result)
}

// Usage
match process_request(&syzygy) {
    Ok(result) => println!("Success: {}", result),
    Err(e) => {
        // Can downcast to specific error types if needed
        if let Some(resource_err) = e.downcast_ref::<ResourceNotFoundError>() {
            eprintln!("Missing resource: {}", resource_err.type_name);
        } else {
            eprintln!("Other error: {}", e);
        }
    }
}
```

### 3. Effect Error Handling

Effects (functions that modify the Syzygy context) should handle their own errors:

```rust
use syzygy::prelude::*;

fn safe_increment_effect(ctx: &mut Syzygy<CounterModel>) {
    // Effects shouldn't panic - handle errors gracefully
    match ctx.get_resource::<Config>() {
        Ok(config) => {
            if config.max_count > ctx.model().count {
                ctx.update(|model| model.count += 1);
            }
        }
        Err(e) => {
            // Log error but don't panic the effect system
            eprintln!("Effect error: {}", e);
        }
    }
}
```

### 4. Async Error Handling

When using async features, errors can be propagated through futures:

```rust
#[cfg(feature = "async")]
use syzygy::prelude::*;

async fn async_operation(syzygy: &Syzygy<MyModel>) -> Result<(), Box<dyn std::error::Error>> {
    let ctx = AsyncContext::create(syzygy.clone()).await?;
    
    ctx.task(|ctx| async move {
        match ctx.get_resource::<ApiClient>() {
            Ok(client) => {
                // Perform async operation...
            }
            Err(e) => {
                eprintln!("Resource error in async task: {}", e);
            }
        }
    });
    
    Ok(())
}
```

## Best Practices

### 1. Don't Ignore Errors

```rust
// ❌ Bad - ignoring potential errors
let config = syzygy.try_resource::<Config>().unwrap();

// ✅ Good - handling the None case
let config = syzygy.try_resource::<Config>()
    .unwrap_or_else(|| Config::default());

// ✅ Better - explicit error handling
let config = match syzygy.get_resource::<Config>() {
    Ok(config) => config,
    Err(e) => {
        eprintln!("Config missing: {}", e);
        return Err("Configuration required".into());
    }
};
```

### 2. Use Appropriate Error APIs

```rust
// ❌ Bad - using panic API when failure is possible
fn risky_function(syzygy: &Syzygy<MyModel>) {
    let config = syzygy.resource::<Config>(); // Might panic!
}

// ✅ Good - using fallible API
fn safe_function(syzygy: &Syzygy<MyModel>) -> Result<(), ResourceNotFoundError> {
    let config = syzygy.get_resource::<Config>()?;
    // ... use config
    Ok(())
}
```

### 3. Provide Context in Effect Errors

```rust
use syzygy::prelude::*;

fn detailed_effect(ctx: &mut Syzygy<MyModel>) {
    match ctx.get_resource::<Database>() {
        Ok(db) => {
            // Perform database operation
        }
        Err(e) => {
            // Provide helpful context
            eprintln!("Effect 'detailed_effect' failed: database resource missing ({})", e);
            // Consider dispatching an error-handling effect
            ctx.dispatch(|ctx| handle_database_error(ctx));
        }
    }
}
```

### 4. Testing Error Conditions

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use syzygy::prelude::*;

    #[test]
    fn test_missing_resource_error() {
        let syzygy = Syzygy::builder()
            .model(TestModel::default())
            .build();

        // Test that the right error is returned
        let result = syzygy.get_resource::<Config>();
        assert!(result.is_err());
        
        let err = result.unwrap_err();
        assert_eq!(err.type_name, std::any::type_name::<Config>());
        assert!(err.to_string().contains("Config"));
    }
}
```

## Summary

Syzygy's error handling is designed to be explicit, composable, and type-safe. By using function-specific error types, you get clear information about what went wrong without having to handle irrelevant error cases. Always prefer the fallible APIs (`get_resource`, `try_resource`) over panic APIs (`resource`) unless you're absolutely certain the resource exists.

For more examples, see the `tests/error_handling_test.rs` file in the codebase.