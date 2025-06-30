# Syzygy Unfucking Roadmap

Your library is a disaster wrapped in abstractions. Here's how we fix this shit, one checkbox at a time.

## Phase 0: Don't Break Production (Safety First, Cowboys)

### Testing Infrastructure
- [x] Write integration tests for ALL current public APIs ✅
  - [x] `Syzygy::builder()` pattern
  - [x] `dispatch()` and `dispatch_sync()`
  - [x] `dispatch_update()`
  - [x] `spawn()` and `task()`
  - [x] Resource storage and retrieval
  - [x] Model access patterns
  - [x] AsyncContext creation and usage
- [x] Create benchmarks for current performance baseline ✅
  - [x] Benchmark model updates per second
  - [x] Benchmark effect dispatch throughput
  - [x] Benchmark resource access time
  - [x] Benchmark memory usage patterns
- [x] Set up CI to run tests + benchmarks on every commit
- [ ] Tag current version as `v0.1.0-pre-unfuck`

### Documentation Baseline
- [ ] Document every public API (even the shitty ones)
- [ ] Add inline examples that actually compile
- [ ] Create `ARCHITECTURE.md` explaining current design
- [ ] Write down every stupid decision and why it was made

## Phase 1: Fix the Embarrassing Shit

### Test Quality
- [x] Remove ALL `thread::sleep()` from tests ✅
  - [x] Replace with `tokio::sync::Notify`
  - [x] Use `tokio::sync::oneshot` for completion signals
  - [x] Add `wait_for_effects()` test helper
  - [x] Create deterministic test harness

### Remove Unsafe Garbage
- [x] Replace `downcast_ref_unchecked()` with safe `downcast_ref()` ✅
  - [x] Benchmark difference (spoiler: it's nothing)
  - [x] Add `#[cfg(debug_assertions)]` type checking if paranoid
- [x] Remove `#![feature(downcast_unchecked)]` ✅
- [x] Remove `#![feature(min_specialization)]` if not used ✅
- [x] Audit all uses of `expect()` - handle errors properly ✅

### Resource System Fixes ✅ COMPLETE
- [x] Stop cloning resources on every access ✅
  - [x] Change `get<T>() -> Option<T>` to `get<T>() -> Option<Arc<T>>`
  - [x] Add `get_cloned<T>() -> Option<T>` for when cloning is needed
  - [x] Update all usages in tests
  - [x] Add deprecation notice on old API
- [x] Fix resource modification ✅
  - [x] Add `update_resource<T, F>(&self, f: F)` for resource access with closures
  - [x] Add `replace_resource<T>(&self, new_value: T)` for replacing resources
  - [x] Add `with_resources_mut()` for batch operations
  - [x] Add comprehensive tests for all resource modification features ✅
- [x] Better error messages ✅
  - [x] "Resource of type X not found" instead of None ✅
  - [x] Add `expect_resource<T>()` with panic message ✅
  - [x] Add `expect_resource_cloned<T>()` with panic message ✅
  - [x] Improve existing `resource()` and `resource_cloned()` panic messages ✅
  - [x] Add comprehensive tests for error message functionality ✅

## Phase 2: Simplify the Architecture

### Trait Consolidation ✅ COMPLETE
- [x] Audit trait usage - find what's actually needed ✅
  - [x] Count usage of `FromContext` vs `IntoContext` - only FromContext used
  - [x] Check if anyone uses these outside the library (no one) ✅
- [x] Merge redundant traits ✅
  - [x] ~~Combine `FromContext` + `IntoContext` into `ContextConvert`~~
  - [x] Use standard `From`/`Into` traits instead of custom ones ✅
  - [x] Create single `StateAccess` trait combining model + resources ✅
  - [x] Add `StateModify` trait for unified read-write operations ✅
- [x] Add sensible defaults ✅
  - [x] Blanket implementations for existing trait combinations ✅
  - [x] Remove redundant custom traits in favor of standard ones ✅
- [x] Add comprehensive tests for new unified traits ✅

### AsyncContext Design ✓ ALREADY GOOD
- [x] ~~Stop cloning the entire model for async contexts~~ **KEEP AS IS** ✅
  - [x] Current snapshot approach is correct - Arc<M::Snapshot>
  - [x] Avoids Arc<Mutex<Model>> (offensive and immoral)
  - [x] Snapshot + Mailbox pattern prevents race conditions
- [x] Make async optional behind feature flag ✅
  - [x] Add `async` feature flag to Cargo.toml ✅
  - [x] Move AsyncContext behind `#[cfg(feature = "async")]` ✅
  - [x] Move `task()` and `spawn()` behind async feature ✅
  - [x] Update tests to conditionally compile async tests ✅
  - [ ] Add documentation about sync-only vs async usage
- [ ] Document the hell out of current design **HIGH PRIORITY**
  - [ ] Explain why snapshots are the right choice
  - [ ] Show how to make `to_snapshot()` cheap with Arc inside models
  - [ ] Add examples of good vs bad model design for snapshots
- [ ] Add performance guidance
  - [ ] Document that `to_snapshot()` should be O(1) when possible
  - [x] Show benchmarks proving no Arc<Mutex> overhead ✅ (async stable ~720-830ns)
  - [ ] Explain async tasks get stale data BY DESIGN

### Effects System Cleanup ✅ COMPLETE 
- [x] Remove pointless abstractions ✅
  - [x] Type alias: `type Effect<M> = Box<dyn FnOnce(&mut Syzygy<M>) + Send>` ✅
  - [x] Clear API surface with proper type exports ✅
  - [x] Function-specific error handling for dispatch operations ✅
- [x] Better effect handling ✅
  - [x] `try_dispatch()` for fallible effect dispatch ✅
  - [x] Proper error types instead of panics ✅
  - [x] Integration with debugging and tracing system ✅

## Phase 3: Add Actually Useful Features

### Error Handling ✅ COMPLETE
- [x] Create proper error types ✅
  - [x] Function-specific error types instead of giant enum ✅
  - [x] `ResourceNotFoundError` for resource lookup failures ✅
  - [x] `DispatchError` for effect dispatch failures ✅
  - [x] `ContextCreationError`, `LockAcquisitionError`, `ResourceReplaceError` ✅
- [x] Make APIs fallible where appropriate ✅
  - [x] `try_resource()` returns Option (existing) ✅
  - [x] `get_resource()` returns Result<T, ResourceNotFoundError> ✅
  - [x] `try_dispatch()` for effects that can fail ✅
  - [x] Each function owns its specific error type ✅
- [x] Follow function-specific error principle ✅
  - [x] Each function defines only errors it can produce ✅
  - [x] No macros needed, just thiserror ✅
  - [x] Errors are composable with standard Result patterns ✅
- [x] Add comprehensive tests for error handling ✅

<!-- FORGOT AOBUT THIS -->
<!-- ### Simple Reactive System
- [ ] Basic change detection (not a fucking graph)
  - [ ] `watch_model()` -> Stream of changes
  - [ ] `watch_resource<T>()` -> Stream when resource changes
  - [ ] Proper subscription cleanup on drop
- [ ] Computed values (keep it simple)
  - [ ] `computed(deps, fn)` with explicit dependencies
  - [ ] Automatic memoization
  - [ ] Clear invalidation rules
- [ ] NO MAGIC - explicit is better than implicit -->

### Developer Experience ✅ COMPLETE
- [x] Debugging tools ✅
  - [x] Effect names/IDs for tracing with EffectTracer ✅
  - [x] Global tracing enablement via `enable_tracing()` ✅
  - [x] Effect execution timeline with start/end times ✅
  - [x] Effect status tracking (Pending/Running/Completed/Failed) ✅
- [x] Metrics and monitoring ✅
  - [x] Effects processed per second via SyzygyMetrics ✅
  - [x] Resource access counting ✅
  - [x] Model update frequency tracking ✅
  - [x] Uptime and performance metrics ✅
- [x] Developer-friendly APIs ✅
  - [x] `print_debug_summary()` for quick debugging ✅
  - [x] Thread-safe global debug state with OnceLock ✅
  - [x] Comprehensive test coverage for debug tools ✅

## Phase 4: The Great Renaming

### Choose a Real Name
- [ ] Brainstorm names that humans can pronounce
  - [ ] `minstate` - minimal state management
  - [ ] `effecty` - effects made easy (meh)
  - [ ] `stated` - your state, handled
  - [ ] `reactor` - reactive state (boring but clear)
  - [ ] `fluxe` - flux but Rusty (kill me)
- [ ] Check crates.io availability
- [ ] Check GitHub availability
- [ ] Domain name (if you're that serious)
- [ ] Ask drunk friends if they can spell it

### Rename Everything
- [ ] Update Cargo.toml with new name
- [ ] Rename main struct from `Syzygy` to something sane
  - [ ] `Runtime`, `StateContainer`, `App`, literally anything else
- [ ] Fix all the weird naming
  - [ ] `model` -> `state` (it's state management FFS)
  - [ ] `effects_bus` -> `effects_channel` or just `channel`
  - [ ] `effects_tx` -> `tx` (we know what it is)
- [ ] Update all imports and docs
- [ ] Add compatibility module with old names (deprecated)

## Phase 5: Documentation That Doesn't Suck

### README Rewrite
- [ ] Explain WTF this library does in 3 sentences max
- [ ] Show simplest possible example that works
- [ ] "Why not X?" section comparing to alternatives
- [ ] Installation instructions that actually work
- [ ] Link to real examples

### Examples That Matter
- [ ] Counter example (hello world of state management)
- [ ] Todo app (because apparently we must)
- [ ] Concurrent data processor (show off threading)
- [ ] Game state example (60fps update loop)
- [ ] Web server state (real world usage)
- [ ] Each example < 100 lines
- [ ] Each example has tests

### Architecture Documentation
- [ ] Draw ASCII diagram of how effects flow
- [ ] Explain the actor model inspiration (if any)
- [ ] Document threading model clearly
- [ ] Performance characteristics and guarantees
- [ ] When NOT to use this library

### Migration Guide
- [ ] "Upgrading from 0.1" guide
- [ ] Automated migration script for common patterns
- [ ] Side-by-side before/after examples
- [ ] FAQ for common issues

## Phase 6: Polish and Ship

### Performance Optimization
- [ ] Profile everything with `cargo flamegraph`
- [ ] Find the actual bottlenecks (not guesses)
- [ ] Optimize only what matters
  - [ ] Zero-copy where possible
  - [ ] Better data structures (FxHashMap is good start)
  - [ ] Batch processing for effects
- [ ] Document performance characteristics

### Feature Flags
- [x] `default = ["async"]` - standard features with async ✅
- [x] `async` - AsyncContext, task(), spawn() methods ✅
- [x] `parallel` - parallel execution using rayon (already exists) ✅
- [ ] `full` - everything including kitchen sink

### Final Cleanup
- [ ] Remove all deprecated APIs
- [ ] Final API review - would drunk you understand?
- [ ] Benchmark against v0.1 - prove improvements
- [ ] Security audit - no unsafe without good reason
- [ ] Add CHANGELOG.md
- [ ] Write release notes

### Release
- [ ] Publish to crates.io as 1.0.0
- [ ] Write blog post: "How I Unfucked My State Library"
- [ ] Post to /r/rust for roasting
- [ ] Handle inevitable bug reports
- [ ] Contemplate life choices

## Continuous Rules

### During Development
- [ ] Every PR must pass all tests
- [ ] Every optimization needs before/after benchmark
- [ ] Every breaking change needs migration notes
- [ ] If it takes > 5 min to explain, it's too complex
- [ ] No abstraction without real use case

### Code Review Checklist
- [ ] Would hungover me understand this?
- [ ] Is this the simplest solution?
- [ ] Did I add unnecessary traits?
- [ ] Can I explain this to a junior dev?
- [ ] Will I hate myself for this in 6 months?

## Quick Wins (Do These Now)

- [x] Fix the sleeping tests (embarrassing) ✅
- [x] Remove unsafe downcast (pointless) ✅
- [x] Add `#[must_use]` annotations ✅
- [ ] Run `cargo clippy -- -W clippy::pedantic`
- [ ] Add basic README if missing
- [x] Set up GitHub CI if not already ✅
- [ ] Add code coverage reporting

## Notes

Remember: Perfect is the enemy of done. This library doesn't need to solve every problem, it needs to solve ONE problem well. State management with effects. That's it. Everything else is wankery.

When in doubt, choose boring. Boring is reliable. Boring is maintainable. Boring is what keeps you sane at 3am when production is on fire.

Now stop reading and start checking boxes, you magnificent disaster.
