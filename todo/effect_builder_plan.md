# EffectBuilder Implementation Plan

Because composing functions is better than enterprise middleware bullshit.

## Overview

EffectBuilder lets users wrap effects with common functionality (timing, tracing, retry) without polluting the core API or adding overhead to the fast path. It's just function composition with a nice API.

## Core Principles

1. **Zero overhead when not used** - Plain `dispatch()` stays fast
2. **Composable** - Stack behaviors like LEGO blocks
3. **Simple as fuck** - It's just wrapping functions
4. **User extensible** - They can add their own wrappers

## Phase 1: Basic Implementation

### Core EffectBuilder Structure

- [ ] Create `effect_builder.rs` module
  ```rust
  pub struct EffectBuilder<M: Model> {
      effect: Box<dyn EffectFn<M>>,
      #[cfg(debug_assertions)]
      debug_info: Option<DebugInfo>,
  }
  ```

- [ ] Implement basic builder methods
  - [ ] `new(effect: impl EffectFn<M>) -> Self`
  - [ ] `build(self) -> impl EffectFn<M>`

- [ ] Add debug info struct (debug builds only)
  ```rust
  #[cfg(debug_assertions)]
  struct DebugInfo {
      name: Option<&'static str>,
      file: &'static str,
      line: u32,
  }
  ```

### Core Wrapping Methods

- [ ] **Timing wrapper**
  ```rust
  pub fn timed(self, name: &'static str) -> Self {
      Self {
          effect: Box::new(move |ctx| {
              let start = Instant::now();
              (self.effect)(ctx);
              let elapsed = start.elapsed();
              #[cfg(debug_assertions)]
              log::debug!("{} took {:?}", name, elapsed);
              // Store metrics somewhere if needed
          }),
          #[cfg(debug_assertions)]
          debug_info: self.debug_info,
      }
  }
  ```

- [ ] **Tracing wrapper**
  ```rust
  pub fn traced(self) -> Self {
      Self {
          effect: Box::new(move |ctx| {
              #[cfg(debug_assertions)]
              {
                  let id = generate_effect_id();
                  log::trace!("Effect {} starting", id);
                  (self.effect)(ctx);
                  log::trace!("Effect {} completed", id);
              }
              #[cfg(not(debug_assertions))]
              (self.effect)(ctx);
          }),
          #[cfg(debug_assertions)]
          debug_info: self.debug_info,
      }
  }
  ```

- [ ] **Named wrapper** (for debugging)
  ```rust
  pub fn named(mut self, name: &'static str) -> Self {
      #[cfg(debug_assertions)]
      {
          let mut info = self.debug_info.unwrap_or_default();
          info.name = Some(name);
          self.debug_info = Some(info);
      }
      self
  }
  ```

### Extension Trait for Ergonomics

- [ ] Create `EffectExt` trait
  ```rust
  pub trait EffectExt<M: Model>: EffectFn<M> + Sized + 'static {
      fn timed(self, name: &'static str) -> EffectBuilder<M> {
          EffectBuilder::new(self).timed(name)
      }
      
      fn traced(self) -> EffectBuilder<M> {
          EffectBuilder::new(self).traced()
      }
      
      fn named(self, name: &'static str) -> EffectBuilder<M> {
          EffectBuilder::new(self).named(name)
      }
  }
  ```

- [ ] Blanket implementation
  ```rust
  impl<M: Model, F: EffectFn<M> + 'static> EffectExt<M> for F {}
  ```

## Phase 2: Integration with Syzygy

### Dispatch Methods

- [ ] Add builder-aware dispatch
  ```rust
  impl<M: Model> Syzygy<M> {
      /// Dispatch with builder pattern
      pub fn dispatch_builder<F>(&self, f: F) 
      where
          F: FnOnce() -> EffectBuilder<M>
      {
          self.dispatch(f().build());
      }
  }
  ```

- [ ] Add convenience methods
  - [ ] `dispatch_timed(name: &str, effect: impl EffectFn<M>)`
  - [ ] `dispatch_traced(effect: impl EffectFn<M>)`

### Performance Considerations

- [ ] Ensure all wrapper functions are `#[inline]`
- [ ] Use `Box` only when necessary
- [ ] Consider using `Arc` for shared debug info
- [ ] Benchmark overhead of each wrapper

## Phase 3: Advanced Features

### Conditional Compilation

- [ ] Make tracing a feature flag
  ```toml
  [features]
  effect-tracing = ["log"]
  effect-metrics = ["metrics"]
  ```

- [ ] Compile out overhead in release
  ```rust
  #[cfg(any(debug_assertions, feature = "effect-tracing"))]
  pub fn traced(self) -> Self { /* ... */ }
  
  #[cfg(not(any(debug_assertions, feature = "effect-tracing")))]
  pub fn traced(self) -> Self { self }
  ```

### Error Handling

- [ ] Add fallible effects support
  ```rust
  pub fn with_error_handling<E>(self, handler: impl Fn(E)) -> Self 
  where
      E: std::error::Error
  {
      // Implementation
  }
  ```

- [ ] Add retry logic (requires fallible effects)
  ```rust
  pub fn with_retry(self, attempts: usize) -> Self {
      // Only if effects can fail
  }
  ```

### Metrics Integration

- [ ] Add metrics collection wrapper
  ```rust
  pub fn with_metrics(self, collector: &MetricsCollector) -> Self {
      // Count effect executions
      // Record timing
      // Track errors
  }
  ```

## Phase 4: Macro Support

### Basic Effect Macro

- [ ] Create `effect!` macro
  ```rust
  #[macro_export]
  macro_rules! effect {
      // Plain effect
      ($effect:expr) => {
          $effect
      };
      
      // Timed effect
      (timed $name:literal => $effect:expr) => {
          EffectBuilder::new($effect).timed($name).build()
      };
      
      // Traced effect
      (traced => $effect:expr) => {
          EffectBuilder::new($effect).traced().build()
      };
      
      // Combined
      (timed $name:literal, traced => $effect:expr) => {
          EffectBuilder::new($effect)
              .timed($name)
              .traced()
              .build()
      };
  }
  ```

- [ ] Add usage examples
- [ ] Test macro hygiene
- [ ] Document in README

### Procedural Macro (Optional)

- [ ] Consider proc macro for better syntax
  ```rust
  #[effect(timed = "user_update", traced)]
  fn update_user(ctx: &mut Syzygy<Model>) {
      // Implementation
  }
  ```

## Phase 5: Testing

### Unit Tests

- [ ] Test each builder method
  - [ ] Timing actually measures time
  - [ ] Tracing outputs correct logs
  - [ ] Named effects have names
  - [ ] Composition order matters

- [ ] Test performance
  - [ ] Benchmark plain effect vs wrapped
  - [ ] Measure memory overhead
  - [ ] Check for allocations

- [ ] Test edge cases
  - [ ] Panicking effects
  - [ ] Very long-running effects
  - [ ] Recursive effects

### Integration Tests

- [ ] Test with real Syzygy usage
- [ ] Test concurrent wrapped effects
- [ ] Test memory usage under load
- [ ] Test debug vs release behavior

## Phase 6: Documentation

### API Documentation

- [ ] Document every public method
- [ ] Add examples to each wrapper
- [ ] Show composition patterns
- [ ] Performance implications

### Guide Documentation

- [ ] "When to use EffectBuilder"
- [ ] "Creating custom wrappers"
- [ ] "Performance considerations"
- [ ] "Debug vs Release behavior"

### Examples

- [ ] Basic timing example
- [ ] Debugging slow effects
- [ ] Custom wrapper example
- [ ] Production usage patterns

## Implementation Order

1. **Week 1: Core Builder**
   - [ ] Basic structure
   - [ ] `timed()` wrapper
   - [ ] Integration with dispatch
   - [ ] Basic tests

2. **Week 2: Full Features**
   - [ ] `traced()` wrapper
   - [ ] `named()` wrapper
   - [ ] Extension trait
   - [ ] Performance benchmarks

3. **Week 3: Polish**
   - [ ] Conditional compilation
   - [ ] Macro support
   - [ ] Documentation
   - [ ] Examples

## Success Criteria

- [ ] Zero overhead for unwrapped effects
- [ ] < 5% overhead for wrapped effects
- [ ] Clean, composable API
- [ ] Works in const contexts where possible
- [ ] No breaking changes to existing API
- [ ] Users can create custom wrappers

## Things to Avoid

- [ ] Don't add too many wrapper methods
- [ ] Don't make it required
- [ ] Don't add complex configuration
- [ ] Don't break the fast path
- [ ] Don't over-engineer

## Quick Prototype

Start with this minimal version:

```rust
// effect_builder.rs
use crate::prelude::*;

pub struct EffectBuilder<M: Model> {
    effect: Box<dyn EffectFn<M>>,
}

impl<M: Model> EffectBuilder<M> {
    pub fn new(effect: impl EffectFn<M> + 'static) -> Self {
        Self {
            effect: Box::new(effect),
        }
    }
    
    #[inline]
    pub fn timed(self, name: &'static str) -> Self {
        Self {
            effect: Box::new(move |ctx| {
                let start = std::time::Instant::now();
                (self.effect)(ctx);
                log::debug!("{} took {:?}", name, start.elapsed());
            }),
        }
    }
    
    pub fn build(self) -> impl EffectFn<M> {
        self.effect
    }
}
```

Test it, benchmark it, then expand from there.

Remember: Start simple, measure everything, only add what's actually useful.