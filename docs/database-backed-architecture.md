# Database-Backed Single-Writer Architecture

## Executive Summary

This document describes an architectural pattern that extends Syzygy's current "app is your database" model to support "database is your database" scenarios while maintaining single-writer guarantees and deterministic event processing. The key insight is that **database operations are effects**, not model mutations, allowing the TEA pattern to coordinate database state through a unified event pipeline.

## Problem Statement

### Current Limitation
Syzygy currently assumes the application model IS the source of truth. This works excellently for desktop applications, games, and CLI tools, but limits adoption for:

- **Web applications** backed by PostgreSQL/MySQL
- **Distributed systems** requiring shared state
- **Offline-first applications** with eventual consistency
- **CQRS/Event Sourcing** architectures
- **Collaborative applications** with conflict resolution

### The Challenge: Maintaining Single-Writer Benefits with External State

Many applications need **both** external persistence AND the benefits of single-writer coordination:

1. **Consistency**: All state changes coordinated through one pipeline
2. **Determinism**: Same events always produce same state transitions  
3. **Testability**: Pure functions with predictable side effects
4. **Error Handling**: Unified error-as-events pattern
5. **Performance**: Local caching with optimistic updates

Traditional approaches sacrifice these benefits by:
- Mixing synchronous and asynchronous state updates
- Splitting error handling between local and database errors
- Creating race conditions between local cache and database
- Making testing complex due to external dependencies

## Solution Overview

### Core Architectural Insight

**Database mutations are effects, not model updates.** The model becomes a **projection** or **cache** of database state, not the source of truth.

This maintains single-writer guarantees while supporting external persistence:

```
Events → Update (Pure) → Commands → Effects (DB Operations) → Events (DB Results) → Update (Cache)
```

### Key Principles

1. **Model as Projection**: Local state mirrors/caches database state
2. **Database Operations as Effects**: All DB writes go through effect pipeline
3. **Event-Driven Synchronization**: DB results flow back as events
4. **Unified Error Handling**: Database errors are events, not exceptions
5. **Single Writer Coordination**: All state changes (local + DB) serialized

## Detailed Architecture

### 1. Model Structure: Projection Pattern

The application model becomes a projection/cache of database state plus local-only state:

```rust
#[derive(Debug)]
struct AppModel {
    // PROJECTION: Cache of database state
    users_cache: HashMap<UserId, User>,
    orders_cache: HashMap<OrderId, Order>,
    last_sync: HashMap<String, Instant>,
    
    // LOCAL STATE: UI and application-specific state
    ui_state: UiState,
    pending_operations: VecDeque<PendingOperation>,
    optimistic_updates: HashMap<OpId, OptimisticUpdate>,
    
    // METADATA: Projection management
    cache_validity: HashMap<String, CacheStatus>,
    error_state: Option<ErrorInfo>,
}

#[derive(Debug)]
enum CacheStatus {
    Fresh,
    Stale { since: Instant },
    Invalid,
    Loading,
}

#[derive(Debug)]
struct PendingOperation {
    id: OpId,
    operation: DatabaseOperation,
    created_at: Instant,
    retry_count: u32,
}
```

### 2. Event Types: Database Lifecycle Events

Events cover the full lifecycle of database operations:

```rust
#[derive(Debug, Clone)]
enum Event {
    // USER-INITIATED ACTIONS
    CreateUser { name: String, email: String },
    UpdateUser { id: UserId, changes: UserPatch },
    DeleteUser { id: UserId },
    PlaceOrder { user_id: UserId, items: Vec<OrderItem> },
    
    // DATABASE LIFECYCLE EVENTS
    DatabaseOperationStarted { op_id: OpId, operation: DatabaseOperation },
    DatabaseOperationCompleted { op_id: OpId, result: DatabaseResult },
    DatabaseOperationFailed { op_id: OpId, error: DatabaseError },
    
    // CACHE SYNCHRONIZATION EVENTS
    DataLoadedFromDatabase { table: String, data: DatabaseData },
    CacheInvalidated { table: String, reason: InvalidationReason },
    CacheSyncCompleted { tables: Vec<String> },
    
    // OPTIMISTIC UPDATE EVENTS
    OptimisticUpdateApplied { op_id: OpId, update: OptimisticUpdate },
    OptimisticUpdateConfirmed { op_id: OpId },
    OptimisticUpdateRolledBack { op_id: OpId, reason: String },
    
    // ERROR RECOVERY EVENTS
    DatabaseConnectionLost,
    DatabaseConnectionRestored,
    ConflictDetected { op_id: OpId, conflict: ConflictInfo },
    RetryLimitExceeded { op_id: OpId, final_error: DatabaseError },
}

#[derive(Debug, Clone)]
enum DatabaseResult {
    UserCreated { id: UserId, user: User },
    UserUpdated { id: UserId, user: User },
    UserDeleted { id: UserId },
    OrderPlaced { id: OrderId, order: Order },
    QueryResult { data: Vec<DatabaseRow> },
    TransactionCommitted { tx_id: TransactionId, affected_rows: u64 },
}
```

### 3. Effect Types: Database Operations as First-Class Effects

All database operations become effects, enabling single-writer coordination:

```rust
#[derive(Debug, Clone)]
enum Effect {
    // BASIC DATABASE OPERATIONS
    DatabaseInsert {
        table: String,
        data: DatabaseRow,
        on_success: Event,
        on_failure: Event,
    },
    
    DatabaseUpdate {
        table: String,
        id: DatabaseId,
        changes: DatabasePatch,
        on_success: Event,
        on_failure: Event,
    },
    
    DatabaseDelete {
        table: String,
        id: DatabaseId,
        on_success: Event,
        on_failure: Event,
    },
    
    DatabaseQuery {
        query: DatabaseQuery,
        on_success: Event,
        on_failure: Event,
    },
    
    // TRANSACTION OPERATIONS
    DatabaseTransaction {
        isolation_level: IsolationLevel,
        operations: Vec<DatabaseOperation>,
        on_success: Event,
        on_failure: Event,
    },
    
    // CACHE MANAGEMENT
    RefreshCache { table: String },
    InvalidateCache { table: String },
    
    // OPTIMISTIC UPDATE MANAGEMENT
    ApplyOptimisticUpdate { update: OptimisticUpdate },
    RollbackOptimisticUpdate { op_id: OpId },
    
    // CONFLICT RESOLUTION
    ResolveConflict { 
        strategy: ConflictResolutionStrategy,
        local_version: DataVersion,
        remote_version: DataVersion,
    },
    
    // CONNECTION MANAGEMENT
    ReconnectToDatabase { retry_interval: Duration },
    
    // OTHER EFFECTS
    HttpRequest { url: String, payload: Vec<u8> },
    SendNotification { user_id: UserId, message: String },
    LogEvent { level: LogLevel, message: String },
}

#[derive(Debug, Clone)]
enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}

#[derive(Debug, Clone)]
enum ConflictResolutionStrategy {
    LastWriteWins,
    FirstWriteWins,
    MergeChanges,
    UserDecision { prompt: String },
}
```

### 4. Update Function: Pure Coordination Logic

The update function coordinates between local projections and database operations:

```rust
fn update(
    event: Event,
    ctx: &mut EventContext<Event, Effect, AppModel>,
) -> Command<Event, Effect> {
    let model = ctx.model_mut();
    
    match event {
        // USER ACTIONS: Apply optimistically, trigger DB operation
        Event::CreateUser { name, email } => {
            let op_id = OpId::new();
            let temp_user = User::new_temporary(&name, &email);
            
            // 1. Apply optimistic update to local state
            let optimistic_update = OptimisticUpdate::UserCreated {
                temp_id: temp_user.id,
                user: temp_user.clone(),
            };
            model.optimistic_updates.insert(op_id, optimistic_update.clone());
            model.users_cache.insert(temp_user.id, temp_user);
            
            // 2. Record pending operation
            model.pending_operations.push_back(PendingOperation {
                id: op_id,
                operation: DatabaseOperation::CreateUser { name: name.clone(), email: email.clone() },
                created_at: Instant::now(),
                retry_count: 0,
            });
            
            // 3. Trigger database operation through effect system
            Command::batch(vec![
                Command::event(Event::OptimisticUpdateApplied { op_id, update: optimistic_update }),
                Command::effect(Effect::DatabaseInsert {
                    table: "users".to_string(),
                    data: DatabaseRow::User { name, email },
                    on_success: Event::DatabaseOperationCompleted {
                        op_id,
                        result: DatabaseResult::UserCreated { id: UserId::placeholder(), user: User::placeholder() }
                    },
                    on_failure: Event::DatabaseOperationFailed {
                        op_id,
                        error: DatabaseError::placeholder()
                    },
                })
            ])
        }
        
        // DATABASE SUCCESS: Confirm optimistic update, update cache
        Event::DatabaseOperationCompleted { op_id, result } => {
            // Remove from pending operations
            model.pending_operations.retain(|op| op.id != op_id);
            
            match result {
                DatabaseResult::UserCreated { id, user } => {
                    // Replace temporary entry with real data
                    if let Some(OptimisticUpdate::UserCreated { temp_id, .. }) = model.optimistic_updates.remove(&op_id) {
                        model.users_cache.remove(&temp_id);
                        model.users_cache.insert(id, user);
                    }
                    
                    Command::event(Event::OptimisticUpdateConfirmed { op_id })
                }
                DatabaseResult::UserUpdated { id, user } => {
                    model.users_cache.insert(id, user);
                    Command::event(Event::OptimisticUpdateConfirmed { op_id })
                }
                _ => Command::none()
            }
        }
        
        // DATABASE FAILURE: Rollback optimistic update, handle error
        Event::DatabaseOperationFailed { op_id, error } => {
            // Remove from pending operations
            model.pending_operations.retain(|op| op.id != op_id);
            
            // Rollback optimistic update
            if let Some(optimistic_update) = model.optimistic_updates.remove(&op_id) {
                match optimistic_update {
                    OptimisticUpdate::UserCreated { temp_id, .. } => {
                        model.users_cache.remove(&temp_id);
                    }
                    OptimisticUpdate::UserUpdated { id, original, .. } => {
                        model.users_cache.insert(id, original);
                    }
                }
            }
            
            // Handle error based on type
            match &error {
                DatabaseError::ConnectionLost => {
                    model.error_state = Some(ErrorInfo::ConnectionLost);
                    Command::effect(Effect::ReconnectToDatabase { retry_interval: Duration::from_secs(5) })
                }
                DatabaseError::ConstraintViolation { .. } => {
                    // User error - don't retry
                    model.error_state = Some(ErrorInfo::UserError(error.clone()));
                    Command::event(Event::OptimisticUpdateRolledBack { op_id, reason: error.to_string() })
                }
                DatabaseError::TransientError { .. } => {
                    // Retry the operation
                    if let Some(mut pending_op) = model.pending_operations.iter_mut().find(|op| op.id == op_id) {
                        pending_op.retry_count += 1;
                        if pending_op.retry_count < 3 {
                            Command::effect(Effect::from(pending_op.operation.clone()))
                        } else {
                            Command::event(Event::RetryLimitExceeded { op_id, final_error: error })
                        }
                    } else {
                        Command::none()
                    }
                }
            }
        }
        
        // CACHE MANAGEMENT: Refresh stale data
        Event::CacheInvalidated { table, reason } => {
            model.cache_validity.insert(table.clone(), CacheStatus::Invalid);
            match reason {
                InvalidationReason::DataChanged => {
                    Command::effect(Effect::RefreshCache { table })
                }
                InvalidationReason::Timeout => {
                    Command::effect(Effect::DatabaseQuery {
                        query: DatabaseQuery::SelectAll { table: table.clone() },
                        on_success: Event::DataLoadedFromDatabase { table: table.clone(), data: DatabaseData::placeholder() },
                        on_failure: Event::DatabaseOperationFailed { op_id: OpId::new(), error: DatabaseError::placeholder() },
                    })
                }
            }
        }
        
        Event::DataLoadedFromDatabase { table, data } => {
            // Update cache with fresh data
            match table.as_str() {
                "users" => {
                    if let DatabaseData::Users(users) = data {
                        model.users_cache.clear();
                        for user in users {
                            model.users_cache.insert(user.id, user);
                        }
                    }
                }
                "orders" => {
                    if let DatabaseData::Orders(orders) = data {
                        model.orders_cache.clear();
                        for order in orders {
                            model.orders_cache.insert(order.id, order);
                        }
                    }
                }
                _ => {}
            }
            
            model.cache_validity.insert(table, CacheStatus::Fresh);
            model.last_sync.insert(table, Instant::now());
            Command::none()
        }
        
        // CONNECTION RECOVERY
        Event::DatabaseConnectionLost => {
            model.error_state = Some(ErrorInfo::ConnectionLost);
            // Pause all database operations
            Command::effect(Effect::ReconnectToDatabase { retry_interval: Duration::from_secs(1) })
        }
        
        Event::DatabaseConnectionRestored => {
            model.error_state = None;
            // Resume pending operations
            let pending_effects: Vec<Command<Event, Effect>> = model.pending_operations
                .iter()
                .map(|op| Command::effect(Effect::from(op.operation.clone())))
                .collect();
            Command::batch(pending_effects)
        }
        
        _ => Command::none()
    }
}
```

### 5. Effect Handler: Database Coordination

The effect handler manages actual database operations and connection lifecycle:

```rust
async fn handle_effects(
    effect: Effect,
    ctx: EffectContext<Event, DatabaseResources>,
) {
    match effect {
        Effect::DatabaseInsert { table, data, on_success, on_failure } => {
            let db = ctx.resource::<DatabasePool>();
            
            match db.insert(&table, data).await {
                Ok(result) => {
                    let success_event = match result {
                        InsertResult::User(user) => {
                            Event::DatabaseOperationCompleted {
                                op_id: OpId::from_context(&ctx),
                                result: DatabaseResult::UserCreated { id: user.id, user }
                            }
                        }
                        InsertResult::Order(order) => {
                            Event::DatabaseOperationCompleted {
                                op_id: OpId::from_context(&ctx),
                                result: DatabaseResult::OrderPlaced { id: order.id, order }
                            }
                        }
                    };
                    let _ = ctx.send_event(success_event);
                }
                Err(e) => {
                    let failure_event = Event::DatabaseOperationFailed {
                        op_id: OpId::from_context(&ctx),
                        error: DatabaseError::from(e)
                    };
                    let _ = ctx.send_event(failure_event);
                }
            }
        }
        
        Effect::DatabaseTransaction { isolation_level, operations, on_success, on_failure } => {
            let db = ctx.resource::<DatabasePool>();
            
            match db.begin_transaction(isolation_level).await {
                Ok(mut tx) => {
                    let mut results = Vec::new();
                    let mut all_succeeded = true;
                    
                    for operation in operations {
                        match tx.execute(operation).await {
                            Ok(result) => results.push(result),
                            Err(e) => {
                                let _ = tx.rollback().await;
                                let _ = ctx.send_event(Event::DatabaseOperationFailed {
                                    op_id: OpId::from_context(&ctx),
                                    error: DatabaseError::TransactionFailed { 
                                        operation: operation.to_string(),
                                        error: e.to_string()
                                    }
                                });
                                all_succeeded = false;
                                break;
                            }
                        }
                    }
                    
                    if all_succeeded {
                        match tx.commit().await {
                            Ok(_) => {
                                let _ = ctx.send_event(Event::DatabaseOperationCompleted {
                                    op_id: OpId::from_context(&ctx),
                                    result: DatabaseResult::TransactionCommitted {
                                        tx_id: tx.id(),
                                        affected_rows: results.iter().map(|r| r.rows_affected()).sum()
                                    }
                                });
                            }
                            Err(e) => {
                                let _ = ctx.send_event(Event::DatabaseOperationFailed {
                                    op_id: OpId::from_context(&ctx),
                                    error: DatabaseError::CommitFailed { error: e.to_string() }
                                });
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = ctx.send_event(Event::DatabaseOperationFailed {
                        op_id: OpId::from_context(&ctx),
                        error: DatabaseError::TransactionStartFailed { error: e.to_string() }
                    });
                }
            }
        }
        
        Effect::RefreshCache { table } => {
            let db = ctx.resource::<DatabasePool>();
            
            match db.select_all(&table).await {
                Ok(data) => {
                    let _ = ctx.send_event(Event::DataLoadedFromDatabase {
                        table,
                        data: DatabaseData::from(data)
                    });
                }
                Err(e) => {
                    let _ = ctx.send_event(Event::DatabaseOperationFailed {
                        op_id: OpId::from_context(&ctx),
                        error: DatabaseError::QueryFailed { 
                            query: format!("SELECT * FROM {}", table),
                            error: e.to_string()
                        }
                    });
                }
            }
        }
        
        Effect::ReconnectToDatabase { retry_interval } => {
            let db = ctx.resource::<DatabasePool>();
            
            loop {
                match db.health_check().await {
                    Ok(_) => {
                        let _ = ctx.send_event(Event::DatabaseConnectionRestored);
                        break;
                    }
                    Err(_) => {
                        tokio::time::sleep(retry_interval).await;
                    }
                }
            }
        }
        
        _ => {
            // Handle other effects (HTTP, notifications, etc.)
        }
    }
}
```

## Benefits of This Architecture

### 1. **Single-Writer Guarantees**
- All state changes (local + database) flow through one serialized pipeline
- No race conditions between local cache and database updates
- Deterministic state transitions regardless of external state

### 2. **Unified Error Handling**
- Database errors are events, not exceptions
- Consistent error handling patterns across local and remote operations
- Error recovery strategies implemented as pure functions

### 3. **Optimistic Updates with Safety**
- Local state updates immediately for responsive UX
- Automatic rollback on database failures
- Conflict detection and resolution through event system

### 4. **Testability**
- Update functions remain pure despite database coordination
- Database operations can be mocked at the effect level
- Complete application behavior testable without external dependencies

### 5. **Performance**
- Local cache provides immediate reads
- Batch database operations through transaction effects
- Intelligent cache invalidation reduces unnecessary queries

### 6. **Resilience**
- Graceful handling of database connection failures
- Automatic retry logic with exponential backoff
- Optimistic updates continue working during disconnections

## Usage Patterns

### 1. **Web Application with PostgreSQL**

```rust
#[derive(Debug)]
struct WebAppModel {
    users: HashMap<UserId, User>,
    sessions: HashMap<SessionId, Session>,
    pending_operations: VecDeque<PendingOp>,
}

let (core, shell) = Syzygy::builder()
    .model(WebAppModel::default())
    .resource(PostgresPool::new("postgres://localhost/myapp"))
    .resource(RedisPool::new("redis://localhost"))
    .update(web_app_update)
    .build();

let shell = shell.with_effect_handler(database_effect_handler);
let mut runner = Runner::new(core, shell);
```

### 2. **CQRS with Event Sourcing**

```rust
#[derive(Debug)]
struct CQRSModel {
    // Read model - projection of events
    current_state: HashMap<AggregateId, AggregateProjection>,
    // Event stream position
    last_processed_event: EventId,
}

enum Event {
    CommandReceived { aggregate_id: AggregateId, command: Command },
    EventPersisted { event_id: EventId, domain_event: DomainEvent },
    ProjectionUpdated { aggregate_id: AggregateId, projection: AggregateProjection },
}

enum Effect {
    PersistEvent { domain_event: DomainEvent },
    UpdateProjection { aggregate_id: AggregateId, event: DomainEvent },
    PublishEvent { domain_event: DomainEvent },
}
```

### 3. **Offline-First Application**

```rust
#[derive(Debug)]
struct OfflineAppModel {
    // Local state (always available)
    local_documents: HashMap<DocId, Document>,
    // Sync state
    pending_sync: Vec<SyncOperation>,
    last_sync: Option<Instant>,
    sync_conflicts: Vec<ConflictInfo>,
}

enum Event {
    DocumentModified { doc_id: DocId, changes: DocumentChanges },
    SyncStarted,
    SyncDataReceived { changes: Vec<RemoteChange> },
    ConflictDetected { doc_id: DocId, local: Document, remote: Document },
    ConflictResolved { doc_id: DocId, resolution: ConflictResolution },
}

enum Effect {
    SaveToLocal { doc_id: DocId, document: Document },
    SyncWithRemote { changes: Vec<LocalChange> },
    ResolveConflict { strategy: ConflictStrategy },
}
```

## Implementation Considerations

### 1. **Database Resource Management**

```rust
#[derive(Clone)]
struct DatabaseResources {
    pool: Arc<DatabasePool>,
    cache: Arc<RwLock<QueryCache>>,
    connection_monitor: Arc<ConnectionMonitor>,
}

// Automatic resource injection in effect handlers
async fn effect_handler(
    effect: Effect,
    pool: &DatabasePool,
    cache: &QueryCache,
    ctx: EffectContext<Event, DatabaseResources>,
) {
    // Resources automatically extracted
}
```

### 2. **Operation ID Management**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct OpId(u64);

impl OpId {
    fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}
```

### 3. **Cache Coherence**

```rust
trait CacheStrategy {
    fn should_refresh(&self, table: &str, last_update: Instant) -> bool;
    fn invalidation_policy(&self) -> InvalidationPolicy;
}

enum InvalidationPolicy {
    TimeToLive(Duration),
    WriteThrough,
    EventDriven,
}
```

### 4. **Conflict Resolution**

```rust
trait ConflictResolver {
    fn resolve_conflict(
        &self,
        local: &DatabaseRow,
        remote: &DatabaseRow,
        strategy: ConflictResolutionStrategy,
    ) -> Result<DatabaseRow, ConflictError>;
}
```

## Migration Strategy

### Phase 1: Basic Database Effects
- Implement database operations as effects
- Add projection pattern support
- Basic optimistic updates

### Phase 2: Advanced Features
- Transaction coordination
- Conflict resolution
- Connection resilience

### Phase 3: Specialized Patterns
- Event sourcing integration
- CQRS support
- Offline-first capabilities

## Conclusion

This architecture extends Syzygy's single-writer TEA pattern to support database-backed applications while maintaining all the benefits of deterministic state management. By treating database operations as effects rather than direct model mutations, we achieve:

1. **Unified Architecture**: Same patterns work for local and database state
2. **Single Writer Benefits**: Consistency, determinism, testability
3. **Real-World Applicability**: Supports web apps, distributed systems, offline-first apps
4. **Graceful Degradation**: Works with or without database connectivity
5. **Performance**: Optimistic updates with intelligent caching

The key insight is that the TEA pattern doesn't require the model to be the source of truth - it only requires that all state changes flow through a single, deterministic pipeline. Database operations become just another category of side effect, coordinated through the same event system that manages all other application concerns.

This makes Syzygy applicable to the full spectrum of applications, from pure local state management to complex distributed systems, all while maintaining the architectural benefits that make TEA so powerful.