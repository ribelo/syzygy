use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::Stream;
use futures::StreamExt;

use crate::command::{Command, TaskLeaseScope};
use crate::process::{ProcessError, ProcessExit, ProcessSpec, ProcessUpdate};

type BoxFutureCommand<E, X> = Pin<Box<dyn Future<Output = Command<E, X>> + 'static>>;
type BoxOptionalFutureCommand<E, X> =
    Pin<Box<dyn Future<Output = Option<Command<E, X>>> + 'static>>;
type BoxStreamCommand<E, X> = Pin<Box<dyn Stream<Item = Command<E, X>> + 'static>>;
type BlockingSpawner<E, X> = Box<dyn FnOnce(crate::runtime::Runtime) -> BoxFutureCommand<E, X>>;
type CooperativeBlockingSpawner<E, X> =
    Box<dyn FnOnce(crate::runtime::Runtime, BlockingCancelToken) -> BoxOptionalFutureCommand<E, X>>;
type ProcessUpdateMapper<E, X> = Box<dyn FnMut(ProcessUpdate) -> Option<Command<E, X>> + 'static>;

#[derive(Clone, Debug, Default)]
pub struct BlockingCancelToken {
    inner: Arc<AtomicBool>,
}

impl BlockingCancelToken {
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.inner.load(Ordering::Relaxed)
    }

    #[must_use]
    pub fn check(&self) -> Option<()> {
        (!self.is_cancelled()).then_some(())
    }

    pub(crate) fn cancel(&self) {
        self.inner.store(true, Ordering::Relaxed);
    }
}

pub struct BlockingTask<E, X> {
    spawn: BlockingSpawner<E, X>,
}

impl<E, X> BlockingTask<E, X>
where
    E: 'static,
    X: 'static,
{
    fn new<F>(work: F) -> Self
    where
        F: FnOnce() -> Command<E, X> + Send + 'static,
        E: Send,
        X: Send,
    {
        Self {
            spawn: Box::new(move |runtime| {
                Box::pin(async move {
                    let handle = runtime.spawn_blocking(work);
                    match handle.await {
                        Ok(command) => command,
                        Err(panic) => std::panic::resume_unwind(panic),
                    }
                })
            }),
        }
    }

    pub(crate) fn into_future(self, runtime: crate::runtime::Runtime) -> BoxFutureCommand<E, X> {
        (self.spawn)(runtime)
    }

    fn map<E2, X2, FE, FX>(
        self,
        scope: Option<TaskLeaseScope>,
        fe: Rc<FE>,
        fx: Rc<FX>,
    ) -> BlockingTask<E2, X2>
    where
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        BlockingTask {
            spawn: Box::new(move |runtime| {
                let future = self.into_future(runtime);
                let scope = scope.clone();
                let fe = Rc::clone(&fe);
                let fx = Rc::clone(&fx);
                Box::pin(async move {
                    let command = future.await;
                    map_command(command, scope.as_ref(), &fe, &fx)
                })
            }),
        }
    }
}

pub struct CooperativeBlockingTask<E, X> {
    spawn: CooperativeBlockingSpawner<E, X>,
}

impl<E, X> CooperativeBlockingTask<E, X>
where
    E: 'static,
    X: 'static,
{
    fn new<F>(work: F) -> Self
    where
        F: FnOnce(BlockingCancelToken) -> Option<Command<E, X>> + Send + 'static,
        E: Send,
        X: Send,
    {
        Self {
            spawn: Box::new(move |runtime, cancel| {
                Box::pin(async move {
                    let cancel_for_task = cancel.clone();
                    let handle = runtime.spawn_blocking(move || work(cancel_for_task));
                    match handle.await {
                        Ok(command) => command,
                        Err(panic) => std::panic::resume_unwind(panic),
                    }
                })
            }),
        }
    }

    pub(crate) fn into_future(
        self,
        runtime: crate::runtime::Runtime,
        cancel: BlockingCancelToken,
    ) -> BoxOptionalFutureCommand<E, X> {
        (self.spawn)(runtime, cancel)
    }

    fn map<E2, X2, FE, FX>(
        self,
        scope: Option<TaskLeaseScope>,
        fe: Rc<FE>,
        fx: Rc<FX>,
    ) -> CooperativeBlockingTask<E2, X2>
    where
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        CooperativeBlockingTask {
            spawn: Box::new(move |runtime, cancel| {
                let future = self.into_future(runtime, cancel);
                let scope = scope.clone();
                let fe = Rc::clone(&fe);
                let fx = Rc::clone(&fx);
                Box::pin(async move {
                    future
                        .await
                        .map(|command| map_command(command, scope.as_ref(), &fe, &fx))
                })
            }),
        }
    }
}

pub struct ProcessTask<E, X> {
    spec: ProcessSpec,
    on_update: ProcessUpdateMapper<E, X>,
}

impl<E, X> ProcessTask<E, X>
where
    E: 'static,
    X: 'static,
{
    fn new<F>(spec: ProcessSpec, on_update: F) -> Self
    where
        F: FnMut(ProcessUpdate) -> Option<Command<E, X>> + 'static,
    {
        Self {
            spec,
            on_update: Box::new(on_update),
        }
    }

    pub(crate) fn into_parts(self) -> (ProcessSpec, ProcessUpdateMapper<E, X>) {
        (self.spec, self.on_update)
    }

    fn map<E2, X2, FE, FX>(
        self,
        scope: Option<TaskLeaseScope>,
        fe: Rc<FE>,
        fx: Rc<FX>,
    ) -> ProcessTask<E2, X2>
    where
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        let (spec, mut on_update) = self.into_parts();
        ProcessTask::new(spec, move |update| {
            on_update(update).map(|command| map_command(command, scope.as_ref(), &fe, &fx))
        })
    }
}

/// Declarative unit of work returned by effect handlers.
pub enum Task<E, X> {
    None,
    Resolved(Command<E, X>),
    Future(BoxFutureCommand<E, X>),
    Stream(BoxStreamCommand<E, X>),
    Process(ProcessTask<E, X>),
    Blocking(BlockingTask<E, X>),
    BlockingCooperative(CooperativeBlockingTask<E, X>),
}

impl<E, X> Task<E, X>
where
    E: 'static,
    X: 'static,
{
    #[must_use]
    pub fn none() -> Self {
        Self::None
    }

    #[must_use]
    pub fn send(event: E) -> Self {
        Self::Resolved(Command::event(event))
    }

    #[must_use]
    pub fn resolved(command: Command<E, X>) -> Self {
        Self::Resolved(command)
    }

    #[must_use]
    pub fn future<Fut>(future: Fut) -> Self
    where
        Fut: Future<Output = Command<E, X>> + 'static,
    {
        Self::Future(Box::pin(future))
    }

    #[must_use]
    pub fn once<Fut>(future: Fut) -> Self
    where
        Fut: Future<Output = Command<E, X>> + 'static,
    {
        Self::future(future)
    }

    #[must_use]
    pub fn stream<S>(stream: S) -> Self
    where
        S: Stream<Item = Command<E, X>> + 'static,
    {
        Self::Stream(Box::pin(stream))
    }

    #[must_use]
    pub fn blocking<F>(work: F) -> Self
    where
        F: FnOnce() -> Command<E, X> + Send + 'static,
        E: Send,
        X: Send,
    {
        Self::Blocking(BlockingTask::new(work))
    }

    #[must_use]
    pub fn blocking_cooperative<F>(work: F) -> Self
    where
        F: FnOnce(BlockingCancelToken) -> Option<Command<E, X>> + Send + 'static,
        E: Send,
        X: Send,
    {
        Self::BlockingCooperative(CooperativeBlockingTask::new(work))
    }

    #[must_use]
    pub fn process<F>(spec: ProcessSpec, map_result: F) -> Self
    where
        F: FnOnce(Result<ProcessExit, ProcessError>) -> Command<E, X> + 'static,
    {
        let mut map_result = Some(map_result);
        Self::Process(ProcessTask::new(spec, move |update| match update {
            ProcessUpdate::Exited(result) => Some(map_result
                .take()
                .expect("process completion mapper must run only once")(
                result
            )),
            ProcessUpdate::Stdout(_) | ProcessUpdate::Stderr(_) => None,
        }))
    }

    #[must_use]
    pub fn process_interactive<F>(spec: ProcessSpec, on_update: F) -> Self
    where
        F: FnMut(ProcessUpdate) -> Option<Command<E, X>> + 'static,
    {
        Self::Process(ProcessTask::new(spec, on_update))
    }

    #[must_use]
    pub fn map<E2, X2, FE, FX>(self, fe: FE, fx: FX) -> Task<E2, X2>
    where
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        match self {
            Self::None => Task::None,
            Self::Resolved(command) => Task::Resolved(command.map(fe, fx)),
            _ => self.map_impl(None, Rc::new(fe), Rc::new(fx)),
        }
    }

    #[must_use]
    pub fn map_scoped<E2, X2, FE, FX>(self, scope: TaskLeaseScope, fe: FE, fx: FX) -> Task<E2, X2>
    where
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        match self {
            Self::None => Task::None,
            Self::Resolved(command) => Task::Resolved(command.map_scoped(&scope, fe, fx)),
            _ => self.map_impl(Some(scope), Rc::new(fe), Rc::new(fx)),
        }
    }

    fn map_impl<E2, X2, FE, FX>(
        self,
        scope: Option<TaskLeaseScope>,
        fe: Rc<FE>,
        fx: Rc<FX>,
    ) -> Task<E2, X2>
    where
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        match self {
            Self::None => Task::None,
            Self::Resolved(command) => match scope {
                Some(scope) => Task::Resolved(command.map_scoped(
                    &scope,
                    |event| fe(event),
                    |effect| fx(effect),
                )),
                None => Task::Resolved(command.map(|event| fe(event), |effect| fx(effect))),
            },
            Self::Future(future) => {
                let fe = Rc::clone(&fe);
                let fx = Rc::clone(&fx);
                let scope = scope.clone();
                Task::Future(Box::pin(async move {
                    let command = future.await;
                    map_command(command, scope.as_ref(), &fe, &fx)
                }))
            }
            Self::Stream(stream) => {
                let fe = Rc::clone(&fe);
                let fx = Rc::clone(&fx);
                let scope = scope.clone();
                Task::Stream(Box::pin(
                    stream.map(move |command| map_command(command, scope.as_ref(), &fe, &fx)),
                ))
            }
            Self::Process(task) => Task::Process(task.map(scope, fe, fx)),
            Self::Blocking(task) => Task::Blocking(task.map(scope, fe, fx)),
            Self::BlockingCooperative(task) => Task::BlockingCooperative(task.map(scope, fe, fx)),
        }
    }

    #[must_use]
    pub fn map_event<E2>(self, f: impl Fn(E) -> E2 + 'static) -> Task<E2, X>
    where
        E2: 'static,
    {
        self.map(f, std::convert::identity)
    }

    #[must_use]
    pub fn map_effect<X2>(self, f: impl Fn(X) -> X2 + 'static) -> Task<E, X2>
    where
        X2: 'static,
    {
        self.map(std::convert::identity, f)
    }
}

fn map_command<E, X, E2, X2, FE, FX>(
    command: Command<E, X>,
    scope: Option<&TaskLeaseScope>,
    fe: &Rc<FE>,
    fx: &Rc<FX>,
) -> Command<E2, X2>
where
    FE: Fn(E) -> E2 + 'static,
    FX: Fn(X) -> X2 + 'static,
{
    match scope {
        Some(scope) => command.map_scoped(scope, |event| fe(event), |effect| fx(effect)),
        None => command.map(|event| fe(event), |effect| fx(effect)),
    }
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::{BlockingCancelToken, Task};
    use crate::command::{Command, CommandStep, TaskLeaseScope};
    use crate::test_store::assert_panic_contains;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ChildEvent;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ChildEffect;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ParentEvent {
        Child(ChildEvent),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ParentEffect {
        Child(ChildEffect),
    }

    #[test]
    fn map_scoped_keeps_abortable_task_and_command_leases_aligned() {
        let lease = crate::command::TaskLease::new();
        let scope = TaskLeaseScope::new();
        let abortable = Command::<ChildEvent, ChildEffect>::abortable(&lease, ChildEffect)
            .map_scoped(&scope, ParentEvent::Child, ParentEffect::Child)
            .into_iter()
            .collect::<Vec<_>>();

        let cancel_task = Task::<ChildEvent, ChildEffect>::resolved(Command::cancel(&lease))
            .map_scoped(scope, ParentEvent::Child, ParentEffect::Child);

        let cancel = match cancel_task {
            Task::Resolved(command) => command.into_iter().collect::<Vec<_>>(),
            _ => panic!("expected resolved task"),
        };

        let abortable_lease = match &abortable[0] {
            CommandStep::Abortable { lease, .. } => lease.clone(),
            _ => panic!("expected abortable command"),
        };

        let cancel_lease = match &cancel[0] {
            CommandStep::Cancel { lease } => lease.clone(),
            _ => panic!("expected cancel command"),
        };

        assert_eq!(abortable_lease, cancel_lease);
    }

    #[test]
    fn map_rejects_abortable_resolved_commands_without_scope() {
        let lease = crate::command::TaskLease::new();

        assert_panic_contains(
            "Command::map cannot safely remap lease-addressed steps",
            || {
                let _ = Task::<ChildEvent, ChildEffect>::resolved(Command::cancel(&lease))
                    .map(ParentEvent::Child, ParentEffect::Child);
            },
        );
    }

    #[test]
    fn map_scoped_future_keeps_abortable_commands() {
        let lease = crate::command::TaskLease::new();
        let mapped = Task::<ChildEvent, ChildEffect>::once(async move { Command::cancel(&lease) })
            .map_scoped(
                TaskLeaseScope::new(),
                ParentEvent::Child,
                ParentEffect::Child,
            );

        let command = match mapped {
            Task::Future(future) => futures::executor::block_on(future),
            _ => panic!("expected future task"),
        };

        let steps = command.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0], CommandStep::Cancel { .. }));
    }

    #[test]
    fn map_scoped_stream_keeps_abortable_commands() {
        let lease = crate::command::TaskLease::new();
        let mapped =
            Task::<ChildEvent, ChildEffect>::stream(futures::stream::iter([Command::cancel(
                &lease,
            )]))
            .map_scoped(
                TaskLeaseScope::new(),
                ParentEvent::Child,
                ParentEffect::Child,
            );

        let commands = match mapped {
            Task::Stream(stream) => futures::executor::block_on(stream.collect::<Vec<_>>()),
            _ => panic!("expected stream task"),
        };

        assert_eq!(commands.len(), 1);
        let steps = commands[0].clone().into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0], CommandStep::Cancel { .. }));
    }

    #[test]
    fn map_scoped_process_task_keeps_process_control_commands() {
        let lease = crate::command::TaskLease::new();
        let mapped = Task::<ChildEvent, ChildEffect>::process_interactive(
            crate::process::ProcessSpec::new("cat"),
            move |_update| Some(Command::process_write(&lease, b"hello")),
        )
        .map_scoped(
            TaskLeaseScope::new(),
            ParentEvent::Child,
            ParentEffect::Child,
        );

        let command = match mapped {
            Task::Process(task) => {
                let (_spec, mut on_update) = task.into_parts();
                on_update(crate::process::ProcessUpdate::Stdout(
                    crate::process::ProcessFrame::Bytes(b"x".to_vec()),
                ))
                .expect("expected mapped process command")
            }
            _ => panic!("expected process task"),
        };

        let steps = command.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0], CommandStep::ProcessWrite { .. }));
    }

    #[test]
    fn blocking_task_maps_commands() {
        let mapped = Task::<ChildEvent, ChildEffect>::blocking(|| Command::event(ChildEvent))
            .map(ParentEvent::Child, ParentEffect::Child);

        let command = match mapped {
            Task::Blocking(task) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                runtime.block_on(task.into_future(runtime.clone()))
            }
            _ => panic!("expected blocking task"),
        };

        let steps = command.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);
        assert!(matches!(
            steps[0],
            CommandStep::Event(ParentEvent::Child(_))
        ));
    }

    #[test]
    fn blocking_cooperative_task_maps_commands() {
        let mapped = Task::<ChildEvent, ChildEffect>::blocking_cooperative(
            |_cancel: BlockingCancelToken| Some(Command::event(ChildEvent)),
        )
        .map(ParentEvent::Child, ParentEffect::Child);

        let command = match mapped {
            Task::BlockingCooperative(task) => {
                let runtime = crate::runtime::Runtime::new().unwrap();
                runtime.block_on(task.into_future(runtime.clone(), BlockingCancelToken::new()))
            }
            _ => panic!("expected cooperative blocking task"),
        };

        let steps = command
            .expect("expected command")
            .into_iter()
            .collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);
        assert!(matches!(
            steps[0],
            CommandStep::Event(ParentEvent::Child(_))
        ));
    }

    #[test]
    fn blocking_cancel_token_check_reports_cancellation() {
        let cancel = BlockingCancelToken::new();

        assert_eq!(cancel.check(), Some(()));
        cancel.cancel();
        assert_eq!(cancel.check(), None);
    }
}
