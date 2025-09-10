# THE GRUG PLAN: Effect Routing & Execution Contexts

*takes a deep breath and exhales smoke*

Alright. So I've read through the current Syzygy implementation. Here's what I see:

Current architecture spawns a goddamn task for every single effect. That's the core problem. Every effect handler gets an `EffectContext` that can spawn async tasks, and the Shell just blindly spawns them all in parallel.

But there's good news: the architecture is actually pretty clean. The separation between Core (sync) and Shell (async) is solid. The Command system for routing is decent. We just need to add effect routing without breaking everything.

Here's my plan to fix this clusterfuck without turning it into a bigger clusterfuck:

## Phase 1: Add Effect Classification (1 Week)
*Start small, prove the concept works*

**Goal**: Add type-level effect classification without changing current behavior.

**What Gets Added**:
```rust
// New trait for effect classification
pub trait EffectKind {
    type Context;  // What execution context this effect needs
}

// Standard execution contexts  
pub struct SingleWriter;    // For database writes, file operations
pub struct ParallelReader;  // For HTTP calls, reads  
pub struct Blocking;        // For CPU-intensive work
pub struct Default;         // Current behavior (spawn everything)
```

**API Changes**:
```rust
// Effects can now declare their execution requirements
#[derive(Debug, Clone)]
enum MyEffect {
    SaveUser(UserData),      // Needs SingleWriter 
    FetchData(Url),          // Can use ParallelReader
    HashPassword(String),    // Needs Blocking context
}

impl EffectKind for MyEffect {
    type Context = SingleWriter; // Default for this effect type
}

// Or per-variant classification (advanced)
impl MyEffect {
    fn execution_context(&self) -> ExecutionContext {
        match self {
            MyEffect::SaveUser(_) => ExecutionContext::SingleWriter,
            MyEffect::FetchData(_) => ExecutionContext::ParallelReader, 
            MyEffect::HashPassword(_) => ExecutionContext::Blocking,
        }
    }
}
```

**Testing**: All existing Syzygy code works unchanged. New effects can opt into classification.

**Complexity**: LOW - Just trait definitions and implementations.

---

## Phase 2: Add Execution Routing (2 Weeks) 
*Route effects to appropriate handlers*

**Goal**: Route effects to specialized execution contexts based on their type.

**What Gets Added**:
```rust
// Effect router in Shell
pub struct EffectRouter<Event, Effect, Resources, H> {
    single_writer: Arc<Mutex<SingleWriterContext<Event, Resources>>>,
    parallel_reader: ParallelReaderContext<Event, Resources>, 
    blocking: BlockingContext<Event, Resources>,
    default_handler: H,  // Fallback to current behavior
}

impl Shell {
    pub fn with_effect_router(self) -> Shell<Event, Effect, Resources, EffectRouter<...>> {
        // Transform shell to use routing
    }
    
    pub fn tick(&mut self) -> Result<bool, ShellError> {
        // Route effects based on classification:
        // - SingleWriter effects go to sequential executor  
        // - ParallelReader effects spawn in parallel
        // - Blocking effects go to thread pool
        // - Unclassified effects use current spawning behavior
    }
}
```

**Backward Compatibility**: Effects without classification use current behavior.

**Configuration**: 
```rust
let shell = shell
    .with_effect_router()
    .with_single_writer_config(SingleWriterConfig { 
        queue_size: 1000,
        timeout: Duration::from_secs(30) 
    })
    .with_blocking_config(BlockingConfig {
        thread_pool_size: 4 
    });
```

**Testing**: 
- Single writer effects execute sequentially
- Parallel reader effects execute concurrently  
- Mixed workloads route correctly
- Performance tests show improvement for write-heavy workloads

**Complexity**: MEDIUM - New execution contexts but clear separation.

---

## Phase 3: Add Resource Control (2 Weeks)
*Add backpressure and resource limits*

**Goal**: Prevent resource exhaustion with configurable limits.

**What Gets Added**:
```rust
pub struct ExecutionLimits {
    max_concurrent_effects: Option<usize>,
    max_queued_effects: Option<usize>, 
    memory_limit: Option<usize>,
    cpu_limit: Option<f32>, // percentage
}

pub struct BackpressureStrategy {
    on_queue_full: BackpressureAction,
    on_memory_limit: BackpressureAction, 
    on_timeout: BackpressureAction,
}

pub enum BackpressureAction {
    Block,           // Wait for capacity  
    Drop,           // Drop new effects
    Event(Event),   // Send event to Core
}
```

**API Changes**:
```rust
let shell = shell
    .with_execution_limits(ExecutionLimits {
        max_concurrent_effects: Some(100),
        max_queued_effects: Some(1000),
        memory_limit: Some(1024 * 1024 * 512), // 512MB
        cpu_limit: Some(0.8), // 80% CPU
    })
    .with_backpressure_strategy(BackpressureStrategy {
        on_queue_full: BackpressureAction::Event(MyEvent::SystemOverloaded),
        on_memory_limit: BackpressureAction::Drop,
        on_timeout: BackpressureAction::Event(MyEvent::EffectTimeout),
    });
```

**Testing**:
- Resource limits prevent system overload
- Backpressure strategies work correctly  
- Performance stays good under normal load
- Graceful degradation under stress

**Complexity**: MEDIUM - Resource monitoring adds overhead but is opt-in.

---

## Phase 4: Advanced Execution Patterns (3 Weeks)
*Support complex execution requirements*

**Goal**: Handle advanced patterns like batching, retries, circuit breakers.

**What Gets Added**:
```rust
pub trait EffectExecutor<Event, Effect, Resources> {
    async fn execute(&self, effect: Effect, ctx: &ExecutionContext<Event, Resources>);
}

// Specialized executors
pub struct BatchingExecutor {
    batch_size: usize,
    batch_timeout: Duration,
}

pub struct RetryExecutor {
    max_attempts: usize, 
    backoff: ExponentialBackoff,
}

pub struct CircuitBreakerExecutor {
    failure_threshold: usize,
    recovery_timeout: Duration,
}
```

**API Changes**:
```rust  
let shell = shell
    .with_custom_executor::<DatabaseWrite, _>(
        BatchingExecutor::new()
            .with_batch_size(10)
            .with_timeout(Duration::from_millis(100))
            .with_retry_policy(RetryPolicy::exponential_backoff(3))
    )
    .with_custom_executor::<HttpRequest, _>(
        CircuitBreakerExecutor::new()
            .with_failure_threshold(5)  
            .with_recovery_timeout(Duration::from_secs(30))
    );
```

**Testing**:
- Batching reduces database load
- Retries handle transient failures  
- Circuit breakers prevent cascade failures
- Performance is measurably better for appropriate workloads

**Complexity**: HIGH - But each executor is independent and opt-in.

---

## Phase 5: Production Readiness (2 Weeks)
*Monitoring, debugging, optimization*

**Goal**: Make the system production-ready with observability.

**What Gets Added**:
```rust
pub struct EffectMetrics {
    effects_processed: Counter,
    execution_time: Histogram,
    queue_depth: Gauge,
    resource_usage: Gauge,
    backpressure_events: Counter,
}

pub struct EffectTracing {
    effect_spans: bool,
    resource_tracking: bool, 
    performance_profiling: bool,
}

#[derive(Debug)]
pub struct EffectDebugInfo {
    effect_id: EffectId,
    execution_context: String,
    queue_position: Option<usize>,
    resource_usage: ResourceUsage,
    execution_time: Option<Duration>,
}
```

**API Changes**:
```rust
let shell = shell
    .with_metrics(EffectMetrics::prometheus()) 
    .with_tracing(EffectTracing::detailed())
    .with_debug_logging(true);

// Debug APIs
let debug_info = shell.effect_debug_info();
let queue_stats = shell.queue_statistics();
let resource_usage = shell.resource_usage();
```

**Testing**:
- Metrics accurately reflect system state
- Tracing helps debug performance issues
- Debug APIs provide actionable information
- Zero overhead when disabled

**Complexity**: MEDIUM - Mostly instrumentation, opt-in features.

---

## Implementation Strategy

### Each Phase:
1. **Write tests first** - Define expected behavior  
2. **Implement minimum viable version** - Get it working
3. **Add comprehensive tests** - Edge cases, performance
4. **Document with examples** - Show real usage patterns
5. **Get feedback** - Make sure it solves real problems

### Backward Compatibility:
- Phase 1-2: 100% backward compatible
- Phase 3-4: Opt-in features only
- Phase 5: Instrumentation doesn't change behavior

### Performance Goals:
- Phase 1: No performance regression
- Phase 2: 2-5x improvement for write-heavy workloads  
- Phase 3: Predictable performance under load
- Phase 4: 5-10x improvement for specialized patterns
- Phase 5: Near-zero overhead monitoring

### API Stability:
- Core event/effect/model APIs: NEVER CHANGE
- Shell configuration: Can evolve with deprecation
- Advanced features: Mark experimental initially

---

## Why This Plan Won't Turn Into Bullshit

1. **Incremental**: Each phase adds value independently
2. **Backward Compatible**: Existing code keeps working
3. **Opt-in Complexity**: Advanced features are optional
4. **Test-Driven**: Behavior is defined by tests, not hopes
5. **Performance-Focused**: Each phase must improve or maintain performance  
6. **Real Problems**: Each phase solves documented issues

The key insight: **Effect classification is the missing piece**. Once we can categorize effects by their execution requirements, everything else follows naturally. Single writers get sequential execution, parallel readers get concurrent execution, blocking work gets thread pools.

And most importantly: **the current simple API stays exactly the same**. Users who don't need advanced features get the same clean experience they have today.

This plan turns the current "spawn everything" chaos into a "route intelligently" system, without sacrificing the simplicity that makes Syzygy useful.

*stubs out cigarette*

Now, you want me to start implementing Phase 1, or do you want to tear apart this plan first?