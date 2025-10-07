# Syzygy Executor Architecture Requirements (EARS)

## Overview
This document defines the requirements for Syzygy's specialized two-trait executor architecture. The system separates async and sync execution concerns to eliminate impedance mismatches and provide optimal performance for different workload patterns.

## Architecture Evolution

### From Single Executor to Specialized Executors
The architecture has evolved from a unified executor approach to a **two-trait system** that clearly separates:
- **Async work**: Cooperative async tasks using dedicated runtimes
- **Sync work**: CPU-bound blocking operations using thread pools

### Key Changes
1. **SingleThreadExecutor is now sync-only** - No longer handles async work
2. **TokioIo/TokioCpu newtype wrappers** - Enable multiple Tokio executors with distinct TypeIds
3. **Clear separation of concerns** - Each executor optimized for specific work types

## Core Architecture Requirements

### REQ-001: Multiple Executor Support
**WHEN** a Syzygy application is configured,
**THE SYSTEM SHALL** support multiple specialized executors running simultaneously.

### REQ-002: Unified Effect Type
**THE SYSTEM SHALL** use a single Effect enum type across all executors,
**WHERE** each executor can handle different variants of the same enum.

### REQ-003: Executor Handle API
**WHEN** an effect needs to be processed by a specific executor,
**THE SYSTEM SHALL** provide an API where executors expose a `handle(effect, handler)` method accessible via `shell.executor::<ExecutorType>().handle()`,
**WHERE** handler is a magic function that automatically extracts the appropriate effect variant.

### REQ-004: Magic Handler Effect Filtering
**WHEN** an executor receives an effect via `handle()`,
**THE SYSTEM SHALL** automatically filter effects to only pass relevant variants to the magic handler function,
**AND** **THE SYSTEM SHALL** only call the handler if the effect matches the handler's expected parameter type.

## Executor Types Requirements

### REQ-005: SingleThreadExecutor (Sync-Only)
**THE SYSTEM SHALL** provide a SingleThreadExecutor,
**WHICH** processes sync effects sequentially on a single dedicated thread,
**AND** **WHICH** guarantees strict FIFO ordering of effect execution,
**AND** **WHICH** provides panic isolation and graceful error handling,
**AND** **WHICH** does NOT implement AsyncExecutor (sync-only by design).

### REQ-006: RayonSyncExecutor
**THE SYSTEM SHALL** provide a RayonSyncExecutor,
**WHICH** processes CPU-intensive sync effects using Rayon's work-stealing parallelism,
**AND** **WHICH** automatically utilizes all available CPU cores,
**AND** **WHICH** provides optimal performance for parallel computations.

### REQ-007: Async Executors (TokioIo & TokioCpu)
**THE SYSTEM SHALL** provide specialized async executor types:
- **TokioIo**: For IO-bound async work with `enable_all()` runtime (network, files)
- **TokioCpu**: For CPU-bound async work with `enable_time()` only (computations)
**WHICH** process async effects using dedicated Tokio runtimes,
**AND** **WHICH** can spawn concurrent async tasks with cancel-on-drop semantics,
**AND** **WHICH** use newtype pattern for distinct TypeId registration.

### REQ-008: Sync Executors
**THE SYSTEM SHALL** provide SyncExecutor types,
**WHICH** process synchronous effects without requiring async runtimes,
**AND** **WHICH** execute effects on appropriate thread pools or single threads,
**AND** **WHICH** maintain their own isolated resource storage,
**AND** **WHICH** never use `spawn_blocking` or other async runtime features.

### REQ-009: IO Runtime Registration Pattern
**THE SYSTEM SHALL** provide IO runtime registration functionality,
**WHICH** allows async executors to register their runtime for IO operations,
**AND** **WHICH** ensures IO operations run on appropriate runtimes while CPU work stays isolated,
**AND** **WHICH** follows InfluxDB's dedicated executor pattern for optimal performance.

### REQ-010: Custom Executor Support
**THE SYSTEM SHALL** allow users to implement custom AsyncExecutor and SyncExecutor types,
**WHERE** custom executors implement the appropriate `trigger(effect, handler)` interface for their execution model.

## Magic Handler Requirements

### REQ-011: Sync Magic Effect Handlers
**THE SYSTEM SHALL** provide SyncMagicEffectHandler trait and implementations,
**WHICH** enable automatic parameter extraction for synchronous effect handlers,
**WHERE** sync handlers return `()` instead of `Future<Output = ()>`,
**AND** **WHERE** sync handlers can extract resources, event senders, and effect variants automatically.

### REQ-012: Async Magic Effect Handlers
**THE SYSTEM SHALL** maintain existing AsyncMagicEffectHandler functionality,
**WHICH** enable automatic parameter extraction for asynchronous effect handlers,
**WHERE** async handlers return `Future<Output = ()>`,
**AND** **WHERE** async handlers can extract resources, event senders, and effect variants automatically.

### REQ-013: Automatic Effect Extraction
**WHEN** a handler function is called via `executor.handle(effect, handler)`,
**THE SYSTEM SHALL** automatically extract the appropriate effect variant based on the handler's parameter type,
**AND** **THE SYSTEM SHALL** only call the handler if the effect matches the expected variant type,
**FOR BOTH** sync and async handler variants.

### REQ-014: Resource Injection
**WHEN** a handler function requires resources,
**THE SYSTEM SHALL** automatically inject resources from the executor's own resource storage based on the handler's parameter types,
**USING** the magic handler system for both sync and async handlers. Handlers run on the
executor they target, and require the appropriate executor to be registered.

### REQ-015: Event Sending
**WHEN** a handler function needs to send events back to the Core,
**THE SYSTEM SHALL** provide automatic injection of event sender functionality,
**USING** the same patterns for both sync and async handlers,
**WHERE** `shell.executor::<ExecutorType>().handle()` returns `()` immediately like current Syzygy architecture.

## API Design Requirements

### REQ-016: Executor Configuration
**THE SYSTEM SHALL** allow configuration of executor parameters,
**SUCH AS** thread pool sizes, queue limits, and runtime selection,
**WHERE** each AsyncExecutor and SyncExecutor type has its own configuration options.

### REQ-017: Executor Registration
**WHEN** building a Syzygy application,
**THE SYSTEM SHALL** provide a builder API for registering multiple AsyncExecutor and SyncExecutor instances using `.executor()` method,
**WHERE** each executor can be configured independently with its own resources,
**AND** **WHERE** executors are accessed by NewType pattern via `shell.executor::<NewTypeWrapper>()` to support multiple executors of the same base type.

### REQ-018: Single Effect Handler Function
**WHEN** the Shell receives effects from the Core,
**THE SYSTEM SHALL** delegate all effect processing to a single effect handler function registered via `Syzygy::builder().handle_effects()`,
**WHERE** the effect handler function receives `(effect, shell)` parameters and uses pattern matching to determine which executor processes which effects,
**AND** **WHERE** this is the only place where effect handling logic is registered.

### REQ-018A: Two-Trait Executor Architecture
**THE SYSTEM SHALL** implement a two-trait executor system with separate `AsyncExecutor<E>` and `SyncExecutor<E>` traits,
**WHERE** async executors handle cooperative async tasks without blocking,
**AND** **WHERE** sync executors handle CPU-bound blocking operations,
**AND** **WHERE** each trait is optimized for its specific execution model without compromises.

### REQ-018B: Shell Tick Architecture
**WHEN** the Shell's `tick()` method is called,
**THE SYSTEM SHALL** delegate tick operations to all registered executors,
**WHERE** each executor implements its own `tick()` method for processing effects,
**AND** **WHERE** the Shell's role is to coordinate executor tick calls rather than process effects directly.

## Runtime Flexibility Requirements

### REQ-019: No Async Requirement
**WHEN** an application doesn't need async functionality,
**THE SYSTEM SHALL** allow complete elimination of async runtimes,
**BY** using only SyncExecutor variants (SingleCoreExecutor, StdThreadExecutor, RayonExecutor).

### REQ-020: Runtime Selection
**WHEN** async functionality is needed,
**THE SYSTEM SHALL** support the tokio runtime as the primary async executor,
**WHERE** runtime selection is configurable per AsyncExecutor instance.

### REQ-021: Mixed Execution Models
**THE SYSTEM SHALL** allow applications to use both AsyncExecutor and SyncExecutor instances simultaneously,
**WHERE** sync effects are handled by SyncExecutors and async effects by AsyncExecutors,
**AND** **WHERE** the distribution system routes effects appropriately.

### REQ-022: Resource Sharing Architecture
**WHEN** using multiple executors,
**THE SYSTEM SHALL** allow shared resources between executors,
**WHERE** resources are responsible for their own internal mutability and thread safety,
**WHERE** the same resource instance can be injected into multiple executors if it implements Clone,
**AND** **WHERE** resource mutation safety is the user's responsibility - the system provides capabilities, users must use them responsibly.

## Performance Requirements

### REQ-023: Executor Specialization
**EACH** AsyncExecutor and SyncExecutor type **SHALL** be optimized for its specific execution model,
**WITHOUT** compromises for supporting other execution patterns.

### REQ-024: Zero-Copy Effect Distribution
**WHEN** distributing effects to multiple executors,
**THE SYSTEM SHALL** avoid unnecessary copying of effect data,
**WHERE** effects can be shared efficiently between both async and sync executors.

### REQ-025: Sync Handler Performance
**SyncMagicEffectHandlers SHALL** have zero allocation overhead for parameter extraction,
**WHERE** synchronous effects are processed with minimal latency compared to async variants.

### REQ-026: Effect Processing Policy
**WHEN** an executor processes effects,
**THE SYSTEM SHALL** process effects based on the executor's internal threading and queuing policies,
**WHERE** backpressure and resource management are handled at the executor implementation level,
**FOR BOTH** async and sync executor variants.

## Integration Requirements

### REQ-027: Core Compatibility
**THE SYSTEM SHALL** maintain full compatibility with existing Syzygy Core functionality,
**WHERE** event processing, model management, and command generation remain unchanged.

### REQ-028: Executor Resource Management
**THE SYSTEM SHALL** provide each AsyncExecutor and SyncExecutor with its own resource storage,
**WHERE** executors can register and access resource instances through the magic handler system,
**AND** **WHERE** the same resource instance can be shared between multiple executors if it implements proper thread safety.

### REQ-029: Error Handling
**WHEN** effects fail during execution,
**THE SYSTEM SHALL** convert errors to events using the existing error-as-events pattern,
**WHERE** effect handlers should never panic and should handle all error cases gracefully by sending appropriate error events.

## Usage Examples Requirements

### REQ-030: Database Writing Pattern (Updated)
**GIVEN** a database write effect,
**WHEN** using SingleThreadExecutor for database operations,
**THE SYSTEM SHALL** process all database writes sequentially using sync handlers,
**ENSURING** no race conditions or transaction conflicts,
**AND** **THE SYSTEM SHALL** guarantee FIFO ordering for write operations.

### REQ-031: CPU Intensive Pattern (Updated)
**GIVEN** a CPU-intensive computation effect,
**WHEN** using RayonSyncExecutor for parallel computations,
**THE SYSTEM SHALL** automatically parallelize the computation across available cores using sync handlers,
**WITHOUT** blocking async executors or single-threaded operations.

### REQ-032: Async I/O Pattern (Updated)
**GIVEN** HTTP request or file I/O effects,
**WHEN** using TokioIo executor with `enable_all()` runtime,
**THE SYSTEM SHALL** process these effects using async handlers with full tokio feature support,
**WITHOUT** blocking synchronous effect processing or CPU-bound async work.

### REQ-032A: Async CPU Pattern (New)
**GIVEN** CPU-bound async computation effects,
**WHEN** using TokioCpu executor with `enable_time()` only runtime,
**THE SYSTEM SHALL** process these effects using lean async handlers without IO overhead,
**PROVIDING** optimal performance for computation-heavy async work.

### REQ-033: Mixed Workload Pattern
**GIVEN** effects requiring different execution models (sync vs async),
**WHEN** both AsyncExecutor and SyncExecutor instances are configured,
**THE SYSTEM SHALL** process each effect type with its optimal executor type,
**WHILE** maintaining overall system responsiveness.

### REQ-034: Updated Basic Usage Pattern
```rust
// Effect enum - same across all executors
enum Effect {
    DatabaseWrite(WriteData),
    ComputeHash(String),
    HttpRequest(Url),
    AsyncCompute(Data),
}

// Executor setup with specialized configurations
let db_executor = SingleThreadExecutor::new()  // Sync-only, FIFO ordering
    .with_resource(Database::connect());
let cpu_executor = RayonSyncExecutor::new();   // Parallel sync work
let io_executor = TokioIo::multi_thread(4);   // IO-bound async (enable_all)
let compute_executor = TokioCpu::multi_thread(8); // CPU async (enable_time only)

// Usage within effect handler function
let app = Syzygy::builder()
    .model(MyModel::default())
    .update(my_update_function)
    .executor(SingleThreadExecutor::new().with_resource(Database::connect()))
    .executor(RayonSyncExecutor::new())
    .executor(TokioIo::multi_thread(4).with_resource(HttpClient::new()))
    .executor(TokioCpu::multi_thread(8))
    .handle_effects(|effect: Effect, shell: Shell| {
        match effect {
            Effect::DatabaseWrite(data) => {
                // Sequential database writes - FIFO ordering guaranteed
                shell.executor::<SingleThreadExecutor>().handle(data, |data: WriteData, db: &Database, sender: EventSender<Event>| {
                    let result = db.write(data);
                    sender.send(Event::DatabaseWriteCompleted(result));
                });
            }
            Effect::ComputeHash(input) => {
                // Parallel CPU computation
                shell.executor::<RayonSyncExecutor>().handle(input, |input: String, sender: EventSender<Event>| {
                    let hash = expensive_hash_computation(input);
                    sender.send(Event::HashComputed(hash));
                });
            }
            Effect::HttpRequest(url) => {
                // IO-bound async work with full tokio features
                shell.executor::<TokioIo>().handle(url, |url: Url, client: &HttpClient, sender: EventSender<Event>| async move {
                    let response = client.get(url).await.unwrap();
                    let data = response.text().await.unwrap();
                    sender.send(Event::HttpResponseReceived(data));
                });
            }
            Effect::AsyncCompute(data) => {
                // CPU-bound async work with minimal runtime overhead
                shell.executor::<TokioCpu>().handle(data, |data: Data, sender: EventSender<Event>| async move {
                    let result = expensive_async_computation(data).await;
                    sender.send(Event::AsyncComputeCompleted(result));
                });
            }
        }
    })
    .build();
```

### REQ-035: Configuration Pattern
```rust
// NewType wrappers for multiple executors of same type
struct DatabaseExecutor(SingleCoreExecutor);
struct CpuExecutor(RayonExecutor);  
struct NetworkExecutor(AsyncExecutor);
struct FileExecutor(AsyncExecutor);

let app = Syzygy::builder()
    .model(MyModel::default())
    .update(my_update_function)
    // All executors registered with .executor() method
    .executor(DatabaseExecutor(SingleCoreExecutor::new()
        .with_queue_size(1000)
        .with_timeout(Duration::from_secs(30))
        .with_resource(Database::connect())))
    .executor(CpuExecutor(RayonExecutor::builder()
        .threads(num_cpus::get())
        .build()))
    .executor(NetworkExecutor(TokioExecutor::multi_thread_io("net", 4)
        .with_max_concurrent(100)
        .with_resource(HttpClient::new())))
    .executor(FileExecutor(TokioExecutor::multi_thread_io("io", 4)
        .with_max_concurrent(50)
        .with_resource(FileSystem::new())))
    .handle_effects(|effect, shell| {
        match effect {
            Effect::DatabaseWrite(data) => {
                shell.executor::<DatabaseExecutor>().handle(data, database_write_handler);
            }
            Effect::ComputeTask(task) => {
                shell.executor::<CpuExecutor>().handle(task, cpu_intensive_handler);
            }
            Effect::HttpRequest(req) => {
                shell.executor::<NetworkExecutor>().handle(req, async_http_handler);
            }
            Effect::FileOperation(op) => {
                shell.executor::<FileExecutor>().handle(op, file_handler);
            }
            Effect::LogAndSave(data) => {
                // One effect can only go to one executor - if you need multiple, call multiple
                shell.executor::<DatabaseExecutor>().handle(data.clone(), save_handler);
                shell.executor::<FileExecutor>().handle(data, log_handler);
            }
        }
    })
    .build();
```

### REQ-036: Resource Integration Pattern
```rust
// Each executor type has its own resource instances
let sync_db_executor = SingleCoreExecutor::new()
    .with_resource(SyncDatabase::connect());  // Sync database connection

let async_http_executor = AsyncExecutor::new()
    .with_resource(HttpClient::new())         // Async HTTP client
    .with_resource(AsyncDatabase::connect()); // Async database connection

// Effect handler function showing resource injection
let app = Syzygy::builder()
    .executor(SingleCoreExecutor::new().with_resource(Database::connect()))
    .executor(AsyncExecutor::new().with_resource(HttpClient::new()).with_resource(Database::connect()))
    .handle_effects(|effect, shell| {
        match effect {
            Effect::DatabaseWrite(write_data) => {
                // Sync handler - resources injected from SingleCoreExecutor's storage
                shell.executor::<SingleCoreExecutor>().handle(write_data, |
                    write_data: DatabaseWrite,
                    db: &Database,          // Injected from executor's resources
                    sender: EventSender<Event>  // Event sender injection
                | {
                    let result = db.write_sync(write_data);
                    sender.send(Event::DatabaseWriteCompleted(result));
                });
            }
            Effect::HttpRequest(request) => {
                // Async handler - resources injected from AsyncExecutor's storage
                shell.executor::<AsyncExecutor>().handle(request, |
                    request: HttpRequest,
                    client: &HttpClient,        // Injected from executor's resources
                    db: &Database,              // Same Database type, different instance or shared
                    sender: EventSender<Event>  // Event sender injection
                | async move {
                    let response = client.get(request.url).await.unwrap();
                    db.log_request_async(&request).await.unwrap();
                    sender.send(Event::RequestCompleted(response));
                });
            }
        }
    });
```

## Non-Functional Requirements

### REQ-037: Backward Compatibility
**THE SYSTEM SHALL** provide a migration path from current single-effect-handler pattern,
**WHERE** existing applications can adopt executor pattern by replacing `.with_effect_handler()` with `.handle_effects()` and adding executor registration,
**AND** **WHERE** single executor usage remains simple for basic applications.

### REQ-038: Documentation
**THE SYSTEM SHALL** provide comprehensive examples and documentation,
**SHOWING** how to choose between AsyncExecutor and SyncExecutor types for different use cases,
**AND** **SHOWING** when to use sync vs async magic handlers.

### REQ-039: Testing Support
**THE SYSTEM SHALL** provide testing utilities for executor-based architectures,
**WHERE** individual AsyncExecutor and SyncExecutor instances and their coordination can be tested independently.

### REQ-040: Error Diagnostics
**WHEN** effects fail during magic handler processing,
**THE SYSTEM SHALL** provide clear diagnostic information,
**SUCH AS** which executor was used and what parameter extraction or execution errors occurred.

## Success Criteria

### REQ-041: Performance Improvement
**THE SYSTEM SHALL** demonstrate measurable performance improvements over single-handler approach,
**FOR** workloads that benefit from specialized sync vs async execution models.

### REQ-042: Simplicity Preservation
**THE SYSTEM SHALL** maintain the simplicity of basic Syzygy usage,
**WHERE** simple applications can use a single AsyncExecutor or SyncExecutor without added complexity.

### REQ-043: Execution Model Optimization
**THE SYSTEM SHALL** enable optimal performance for different effect types,
**WHERE** CPU-intensive effects use SyncExecutors and I/O-intensive effects use AsyncExecutors,
**WITHOUT** forcing async overhead on purely synchronous workloads.

### REQ-044: Flexibility Achievement
**THE SYSTEM SHALL** enable applications to scale from single-threaded embedded systems to high-performance server applications,
**BY** choosing appropriate combinations of AsyncExecutor and SyncExecutor instances.

### REQ-045: Typical Usage Patterns
**THE SYSTEM SHALL** be designed for typical applications that use 1-3 executors at most,
**WHERE** common patterns include single executor for simple apps, or combinations like RayonExecutor for CPU work and SingleCoreExecutor for database writes,
**AND** **WHERE** the API remains ergonomic even when registering multiple executors via repeated `.executor()` calls.
