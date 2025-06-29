# Syzygy Unfucking Roadmap

Your library is a disaster wrapped in abstractions. Here's how we fix this shit, one checkbox at a time.

## Phase 0: Don't Break Production (Safety First, Cowboys)

### Testing Infrastructure
- [ ] Write integration tests for ALL current public APIs
  - [ ] `Syzygy::builder()` pattern
  - [ ] `dispatch()` and `dispatch_sync()`
  - [ ] `dispatch_update()`
  - [ ] `spawn()` and `task()` 
  - [ ] Resource storage and retrieval
  - [ ] Model access patterns
  - [ ] AsyncContext creation and usage
- [ ] Create benchmarks for current performance baseline
  - [ ] Benchmark model updates per second
  - [ ] Benchmark effect dispatch throughput
  - [ ] Benchmark resource access time
  - [ ] Benchmark memory usage patterns
- [ ] Set up CI to run tests + benchmarks on every commit
- [ ] Tag current version as `v0.1.0-pre-unfuck`

### Documentation Baseline
- [ ] Document every public API (even the shitty ones)
- [ ] Add inline examples that actually compile
- [ ] Create `ARCHITECTURE.md` explaining current design
- [ ] Write down every stupid decision and why it was made

## Phase 1: Fix the Embarrassing Shit

### Test Quality
- [ ] Remove ALL `thread::sleep()` from tests
  - [ ] Replace with `tokio::sync::Notify`
  - [ ] Use `tokio::sync::oneshot` for completion signals
  - [ ] Add `wait_for_effects()` test helper
  - [ ] Create deterministic test harness
- [ ] Add property-based tests with `proptest`
  - [ ] Concurrent effect ordering
  - [ ] Resource type safety
  - [ ] Model consistency under load
- [ ] Add stress tests
  - [ ] 10k effects dispatched
  - [ ] 1k concurrent resource accesses
  - [ ] Mixed read/write patterns

### Remove Unsafe Garbage
- [ ] Replace `downcast_ref_unchecked()` with safe `downcast_ref()`
  - [ ] Benchmark difference (spoiler: it's nothing)
  - [ ] Add `#[cfg(debug_assertions)]` type checking if paranoid
- [ ] Remove `#![feature(downcast_unchecked)]`
- [ ] Remove `#![feature(min_specialization)]` if not used
- [ ] Audit all uses of `expect()` - handle errors properly

### Resource System Fixes
- [ ] Stop cloning resources on every access
  - [ ] Change `get<T>() -> Option<T>` to `get<T>() -> Option<Arc<T>>`
  - [ ] Add `get_cloned<T>() -> Option<T>` for when cloning is needed
  - [ ] Update all usages in tests
  - [ ] Add deprecation notice on old API
- [ ] Fix resource modification
  - [ ] Add `update_resource<T, F>(&self, f: F)` for in-place updates
  - [ ] Add resource versioning (simple generation counter)
  - [ ] Add `resources_mut()` for batch updates
- [ ] Better error messages
  - [ ] "Resource of type X not found" instead of None
  - [ ] Add `expect_resource<T>()` with panic message

## Phase 2: Simplify the Architecture

### Trait Consolidation
- [ ] Audit trait usage - find what's actually needed
  - [ ] Count usage of `FromContext` vs `IntoContext`
  - [ ] Check if anyone uses these outside the library
- [ ] Merge redundant traits
  - [ ] Combine `FromContext` + `IntoContext` into `ContextConvert`
  - [ ] Or just use standard `From`/`Into` traits FFS
  - [ ] Create single `StateAccess` trait combining model + resources
- [ ] Add sensible defaults
  - [ ] Default implementations where possible
  - [ ] Derive macros for common patterns
- [ ] Deprecate old traits (keep for compatibility)

### AsyncContext Unfucking
- [ ] Stop cloning the entire model for async contexts
  - [ ] Use `Arc<RwLock<Model>>` or similar
  - [ ] Make snapshots lazy - only create when accessed
  - [ ] Add `snapshot_mode` parameter to control behavior
- [ ] Better async ergonomics
  - [ ] `AsyncContext::with_snapshot()` for explicit snapshots
  - [ ] `AsyncContext::shared()` for shared model access
  - [ ] Clear documentation on when to use which
- [ ] Fix the lifecycle mess
  - [ ] Clear ownership rules
  - [ ] Prevent accidental model divergence

### Effects System Cleanup
- [ ] Remove pointless abstractions
  - [ ] Type alias: `type Effect<M> = Box<dyn FnOnce(&mut Syzygy<M>) + Send>`
  - [ ] Expose the actual channel: `effects_channel() -> &Sender<Effect<M>>`
  - [ ] Move convenience methods to extension trait
- [ ] Better effect handling
  - [ ] `handle_effects_async()` that doesn't block
  - [ ] `handle_effects_with_limit(n)` for batching
  - [ ] Effect priorities or ordering guarantees
- [ ] Error handling for effects
  - [ ] `try_dispatch()` for fallible effects
  - [ ] Dead letter queue for failed effects
  - [ ] Effect retry policies

## Phase 3: Add Actually Useful Features

### Error Handling
- [ ] Create proper error types
  - [ ] `SyzygyError` enum with real variants
  - [ ] `ResourceNotFound { type_name: &'static str }`
  - [ ] `ModelUpdateFailed { reason: String }`
  - [ ] `EffectDispatchFailed { .. }`
- [ ] Make APIs fallible where appropriate
  - [ ] `try_resource()` already exists, use the pattern
  - [ ] `try_dispatch()` for effects that can fail
  - [ ] `try_update()` for model updates
- [ ] Add error context and recovery
  - [ ] `.context()` for adding error context
  - [ ] Recovery strategies for common failures

### Simple Reactive System
- [ ] Basic change detection (not a fucking graph)
  - [ ] `watch_model()` -> Stream of changes
  - [ ] `watch_resource<T>()` -> Stream when resource changes
  - [ ] Proper subscription cleanup on drop
- [ ] Computed values (keep it simple)
  - [ ] `computed(deps, fn)` with explicit dependencies
  - [ ] Automatic memoization
  - [ ] Clear invalidation rules
- [ ] NO MAGIC - explicit is better than implicit

### Developer Experience
- [ ] Debugging tools
  - [ ] Effect names/IDs for tracing
  - [ ] `trace_effects()` mode with full logging
  - [ ] Effect execution timeline
- [ ] Metrics and monitoring
  - [ ] Effects processed per second
  - [ ] Queue depth over time
  - [ ] Resource access patterns
  - [ ] Model update frequency
- [ ] Better panics
  - [ ] Custom panic handler with context
  - [ ] "This panic occurred while processing effect X"

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
- [ ] `default = ["std"]` - standard features
- [ ] `minimal` - core only, no async
- [ ] `full` - everything including kitchen sink
- [ ] `no_std` support (if masochistic enough)

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

- [ ] Fix the sleeping tests (embarrassing)
- [ ] Remove unsafe downcast (pointless)
- [ ] Add `#[must_use]` annotations
- [ ] Run `cargo clippy -- -W clippy::pedantic`
- [ ] Add basic README if missing
- [ ] Set up GitHub CI if not already
- [ ] Add code coverage reporting

## Notes

Remember: Perfect is the enemy of done. This library doesn't need to solve every problem, it needs to solve ONE problem well. State management with effects. That's it. Everything else is wankery.

When in doubt, choose boring. Boring is reliable. Boring is maintainable. Boring is what keeps you sane at 3am when production is on fire.

Now stop reading and start checking boxes, you magnificent disaster.