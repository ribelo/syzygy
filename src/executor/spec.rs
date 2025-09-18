use futures_util::future::{BoxFuture, FutureExt};
use futures_util::stream::{BoxStream, StreamExt};

use std::future::Future;

use super::{AsyncExecutor, ExecutorError};
use crate::effect_context::EffectContext;
use crate::error::ShellError;
use std::any::TypeId;

fn missing_executor(kind: &'static str, exec: TypeId) -> ShellError {
    #[cfg(feature = "tracing")]
    tracing::error!(
        ?exec,
        kind,
        "Missing executor; effect could not be scheduled"
    );

    // Log the error but don't panic - allow the system to continue
    #[cfg(not(feature = "tracing"))]
    eprintln!("Missing {kind} executor for type {exec:?}; effect could not be scheduled");

    ShellError::TaskSpawnFailed(format!("Missing {kind} executor for type {exec:?}"))
}

/// Unified effect output: a single event, multiple events, or none
#[derive(Debug, PartialEq)]
pub enum Outcome<E> {
    None,
    Event(E),
    Events(Vec<E>),
}

// Primary implementation - single events are the most common case
impl<E> From<E> for Outcome<E> {
    fn from(event: E) -> Self {
        Outcome::Event(event)
    }
}

// Support for Result types - convert Ok to Event, Err to None (for now)
impl<E, Err> From<Result<E, Err>> for Outcome<E> {
    fn from(result: Result<E, Err>) -> Self {
        match result {
            Ok(event) => Outcome::Event(event),
            Err(_) => Outcome::None,
        }
    }
}

// Vec<E> - for multiple events
impl<E> From<Vec<E>> for Outcome<E> {
    fn from(events: Vec<E>) -> Self {
        if events.is_empty() {
            Outcome::None
        } else {
            Outcome::Events(events)
        }
    }
}

// Option<E> - for conditional events
impl<E> From<Option<E>> for Outcome<E> {
    fn from(opt: Option<E>) -> Self {
        match opt {
            Some(event) => Outcome::Event(event),
            None => Outcome::None,
        }
    }
}

// REMOVE the From<()> implementation - it conflicts with From<E> when E = ()
// Users should use Task::none() or vec![] for empty outcomes

/// Object-safe factory to start a task using an `EffectContext`.
///
/// This allows storing `FnOnce(EffectContext<E,R>) -> Fut` as a trait object
/// by wrapping it in a struct holding `Option<F>`.
type FutFactory<E, R> =
    Box<dyn FnOnce(EffectContext<E, R>) -> BoxFuture<'static, Outcome<E>> + Send>;

type SyncFactory<E, R> = Box<dyn FnOnce(EffectContext<E, R>) -> Outcome<E> + Send>;

type StreamFactory<E, R> = Box<dyn FnOnce(EffectContext<E, R>) -> BoxStream<'static, E> + Send>;

/// Declarative effect plan produced by sync handlers.
pub enum Task<E, R = ()> {
    Events(Vec<E>),
    Future {
        exec: TypeId,
        task: FutFactory<E, R>,
    },

    /// Run a synchronous task on a Sync executor (e.g., Rayon, single-thread)
    Sync {
        exec: TypeId,
        task: SyncFactory<E, R>,
    },
    /// Stream events from an async executor
    Stream {
        exec: TypeId,
        factory: StreamFactory<E, R>,
    },
}

impl<E, R> Task<E, R>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    #[must_use]
    pub fn events<I>(events: I) -> Self
    where
        I: IntoIterator<Item = E>,
    {
        Self::Events(events.into_iter().collect())
    }

    #[must_use]
    pub fn none() -> Self {
        Self::Events(vec![])
    }

    #[must_use]
    pub fn event(event: E) -> Self {
        Self::Events(vec![event])
    }

    /// Create a task from an async block
    pub fn async_task<Exec, Fut>(future: Fut) -> Self
    where
        Exec: AsyncExecutor<E> + 'static,
        Fut: Future<Output = Outcome<E>> + Send + 'static,
    {
        Self::Future {
            exec: TypeId::of::<Exec>(),
            task: Box::new(move |_ctx| Box::pin(future)),
        }
    }

    /// Create a task with context access
    pub fn async_task_with<Exec, F, Fut>(f: F) -> Self
    where
        Exec: AsyncExecutor<E> + 'static,
        F: FnOnce(EffectContext<E, R>) -> Fut + Send + 'static,
        Fut: Future<Output = Outcome<E>> + Send + 'static,
    {
        Self::Future {
            exec: TypeId::of::<Exec>(),
            task: Box::new(move |ctx| Box::pin(f(ctx))),
        }
    }
}

/// Forward an `Outcome` to the event channel
fn forward_outcome<E>(event_tx: &crossbeam_channel::Sender<E>, outcome: Outcome<E>) {
    match outcome {
        Outcome::None => {}
        Outcome::Event(event) => {
            let _ = event_tx.send(event);
        }
        Outcome::Events(events) => {
            for event in events {
                let _ = event_tx.send(event);
            }
        }
    }
}

/// Drive a `Task` by spawning appropriate work on executors and
/// forwarding produced events to Core via the supplied `EffectContext`.
pub(crate) fn drive_spec<E, R>(
    spec: Task<E, R>,
    ctx: EffectContext<E, R>,
    event_tx: crossbeam_channel::Sender<E>,
) -> Result<BoxFuture<'static, ()>, ShellError>
where
    E: Send + 'static,
    R: Send + Sync + 'static,
{
    match spec {
        Task::Events(events) => Ok(async move {
            for e in events {
                let _ = event_tx.send(e);
            }
        }
        .boxed()),
        Task::Future { exec, task } => {
            let exec_ref = ctx
                .executors()
                .async_exec_by_key(exec)
                .ok_or_else(|| missing_executor("async", exec))?;

            let ctx_for_task = ctx.clone();
            let event_tx = event_tx.clone();

            Ok(async move {
                let fut = Box::pin(async move { (task)(ctx_for_task).await });
                match exec_ref.spawn_future(fut).await {
                    Ok(outcome) => forward_outcome(&event_tx, outcome),
                    Err(
                        ExecutorError::WorkerGone
                        | ExecutorError::Panic { .. }
                        | ExecutorError::Cancelled,
                    ) => {}
                }
            }
            .boxed())
        }
        Task::Sync { exec, task } => {
            let exec_ref = ctx
                .executors()
                .sync_exec_by_key(exec)
                .ok_or_else(|| missing_executor("sync", exec))?;

            let ctx_for_job = ctx.clone();
            let event_tx = event_tx.clone();

            Ok(async move {
                let job = Box::new(move || (task)(ctx_for_job));
                match exec_ref.spawn_sync(job).await {
                    Ok(output) => forward_outcome(&event_tx, output),
                    Err(
                        ExecutorError::WorkerGone
                        | ExecutorError::Panic { .. }
                        | ExecutorError::Cancelled,
                    ) => { /* ignore or log */ }
                }
            }
            .boxed())
        }
        Task::Stream { exec, factory } => {
            let exec_ref = ctx
                .executors()
                .async_exec_by_key(exec)
                .ok_or_else(|| missing_executor("async-stream", exec))?;

            let ctx_for_stream = ctx.clone();
            let event_tx = event_tx.clone();

            Ok(async move {
                let stream = (factory)(ctx_for_stream);
                let fut = async move {
                    use futures::pin_mut;
                    pin_mut!(stream);
                    while let Some(event) = stream.next().await {
                        let _ = event_tx.send(event);
                    }
                    Outcome::None
                }
                .boxed();

                match exec_ref.spawn_future(fut).await {
                    Ok(_)
                    | Err(
                        ExecutorError::WorkerGone
                        | ExecutorError::Panic { .. }
                        | ExecutorError::Cancelled,
                    ) => { /* ignore or log */ }
                }
            }
            .boxed())
        }
    }
}

#[cfg(test)]
mod outcome_tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum TestEvent {
        A,
        B(i32),
        C { value: String },
    }

    #[test]
    fn test_from_single_event() {
        let outcome: Outcome<TestEvent> = TestEvent::A.into();
        match outcome {
            Outcome::Event(TestEvent::A) => {}
            _ => panic!("Expected Event(A), got {outcome:?}"),
        }

        let outcome: Outcome<TestEvent> = TestEvent::B(42).into();
        match outcome {
            Outcome::Event(TestEvent::B(42)) => {}
            _ => panic!("Expected Event(B(42)), got {outcome:?}"),
        }
    }

    #[test]
    fn test_from_vec_events() {
        // Empty vec becomes None
        let outcome: Outcome<TestEvent> = vec![].into();
        match outcome {
            Outcome::None => {}
            _ => panic!("Expected None, got {outcome:?}"),
        }

        // Single element vec becomes Events (not Event)
        let outcome: Outcome<TestEvent> = vec![TestEvent::A].into();
        match outcome {
            Outcome::Events(events) => {
                assert_eq!(events.len(), 1);
                assert_eq!(events[0], TestEvent::A);
            }
            _ => panic!("Expected Events, got {outcome:?}"),
        }

        // Multiple elements
        let outcome: Outcome<TestEvent> =
            vec![TestEvent::A, TestEvent::B(1), TestEvent::B(2)].into();
        match outcome {
            Outcome::Events(events) => {
                assert_eq!(events.len(), 3);
                assert_eq!(events[0], TestEvent::A);
                assert_eq!(events[1], TestEvent::B(1));
                assert_eq!(events[2], TestEvent::B(2));
            }
            _ => panic!("Expected Events, got {outcome:?}"),
        }
    }

    #[test]
    fn test_from_option() {
        // None becomes Outcome::None
        let outcome: Outcome<TestEvent> = None.into();
        match outcome {
            Outcome::None => {}
            _ => panic!("Expected None, got {outcome:?}"),
        }

        // Some becomes Event
        let outcome: Outcome<TestEvent> = Some(TestEvent::C {
            value: "test".to_string(),
        })
        .into();
        match outcome {
            Outcome::Event(TestEvent::C { value }) => {
                assert_eq!(value, "test");
            }
            _ => panic!("Expected Event(C), got {outcome:?}"),
        }
    }

    #[test]
    fn test_outcome_with_unit_type() {
        // This test verifies that we can still work with () as the event type
        // even though we removed From<()>
        let outcome: Outcome<()> = ().into(); // This now uses From<E> where E = ()
        match outcome {
            Outcome::Event(()) => {} // () is treated as an event
            _ => panic!("Expected Event(()), got {outcome:?}"),
        }

        // For None, use vec![] or Task::none()
        let outcome: Outcome<()> = vec![].into();
        match outcome {
            Outcome::None => {}
            _ => panic!("Expected None, got {outcome:?}"),
        }
    }

    #[test]
    fn test_ergonomic_task_creation() {
        // Test that our From impls work well with Task methods

        // Direct event return - test the conversion directly
        let outcome: Outcome<TestEvent> = TestEvent::A.into();
        assert!(matches!(outcome, Outcome::Event(TestEvent::A)));

        // Vec return - test the conversion directly
        let outcome: Outcome<TestEvent> = vec![TestEvent::A, TestEvent::B(1)].into();
        assert!(matches!(outcome, Outcome::Events(_)));

        // Option return - test the conversion directly
        let outcome: Outcome<TestEvent> = Some(TestEvent::A).into();
        assert!(matches!(outcome, Outcome::Event(TestEvent::A)));

        // Empty vec for None - test the conversion directly
        let outcome: Outcome<TestEvent> = Vec::<TestEvent>::new().into();
        assert!(matches!(outcome, Outcome::None));
    }

    #[test]
    fn test_task_creation_methods() {
        // Test async_task creates task
        let task: Task<TestEvent, ()> = Task::async_task::<
            crate::executor::InlineAsync<TestEvent>,
            _,
        >(async move { Outcome::Event(TestEvent::A) });

        match task {
            Task::Future { .. } => {
                // Task created successfully
            }
            _ => panic!("Expected Future task"),
        }

        // Test async_task_with creates task
        let task: Task<TestEvent, ()> =
            Task::async_task_with::<crate::executor::InlineAsync<TestEvent>, _, _>(
                |_ctx| async move { Outcome::Event(TestEvent::A) },
            );

        match task {
            Task::Future { .. } => {
                // Task created successfully
            }
            _ => panic!("Expected Future task"),
        }
    }
}
