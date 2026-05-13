use std::any::TypeId;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::process::ExitStatus;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::channel::oneshot;
use futures::future::{select_all, LocalBoxFuture};
use futures::io::AsyncWrite;
use futures::stream::StreamExt;

use crate::activity::Activity;
use crate::command::{Command, CommandStep, TaskLease, TaskLeaseWeak};
use crate::core::EventSender;
use crate::error::ShellError;
use crate::executor::{BlockingCancelToken, Task};
use crate::extract::EffectContext;
use crate::process::{
    control_channel, ProcessControlMessage, ProcessControlReceiver, ProcessControlSender,
    ProcessError, ProcessFrame, ProcessInput, ProcessOutputDriver, ProcessSpec,
    ProcessTerminationPolicy, ProcessUpdate,
};
use crate::subscription::{
    ErasedSubscriptionMapper, Subscription, SubscriptionDrivers, SubscriptionEntry,
    SubscriptionKey, SubscriptionSpec,
};
use crate::syzygy::{DeferredEventOverflowPolicy, UnhandledEffectPolicy};

pub(crate) type EffectHandlerFn<E, X, Resources> =
    Rc<dyn for<'a> Fn(X, &EffectContext<'a, Resources>) -> Result<Task<E, X>, ShellError>>;
pub(crate) type BootHandlerFn<E, X, M> = Box<dyn Fn(&M) -> Command<E, X>>;
pub(crate) type SubscriptionHandlerFn<E, X, M> =
    Rc<dyn Fn(&crate::extract::SubscriptionContext<M>) -> Subscription<E, X>>;

pub(crate) struct LifecycleHandlers<E, X, M>
where
    E: 'static,
    X: 'static,
{
    boot_handler: Option<BootHandlerFn<E, X, M>>,
    subscription_handler: Option<SubscriptionHandlerFn<E, X, M>>,
}

impl<E, X, M> LifecycleHandlers<E, X, M>
where
    E: 'static,
    X: 'static,
{
    #[must_use]
    pub(crate) fn none() -> Self {
        Self {
            boot_handler: None,
            subscription_handler: None,
        }
    }

    #[must_use]
    pub(crate) fn new(
        boot_handler: Option<BootHandlerFn<E, X, M>>,
        subscription_handler: Option<SubscriptionHandlerFn<E, X, M>>,
    ) -> Self {
        Self {
            boot_handler,
            subscription_handler,
        }
    }
}

enum TaskCancelHandle {
    Blocking(BlockingCancelToken),
    Process(Option<oneshot::Sender<()>>),
    Subscription(Option<oneshot::Sender<()>>),
}

impl TaskCancelHandle {
    fn request_cancel(&mut self) {
        match self {
            Self::Blocking(cancel) => cancel.cancel(),
            Self::Process(cancel) | Self::Subscription(cancel) => {
                if let Some(cancel) = cancel.take() {
                    let _ = cancel.send(());
                }
            }
        }
    }
}

enum TaskHandleKind {
    AbortOnDrop,
    WaitForCompletion { cancel: Option<TaskCancelHandle> },
}

struct TaskHandle {
    join: crate::runtime::JoinHandle<()>,
    kind: TaskHandleKind,
}

impl TaskHandle {
    fn abort_on_drop(join: crate::runtime::JoinHandle<()>) -> Self {
        Self {
            join,
            kind: TaskHandleKind::AbortOnDrop,
        }
    }

    fn wait_for_completion(
        join: crate::runtime::JoinHandle<()>,
        cancel: Option<TaskCancelHandle>,
    ) -> Self {
        Self {
            join,
            kind: TaskHandleKind::WaitForCompletion { cancel },
        }
    }

    #[must_use]
    fn is_finished(&self) -> bool {
        self.join.is_finished()
    }

    fn request_cancel(mut self) -> Option<Self> {
        match &mut self.kind {
            TaskHandleKind::AbortOnDrop => None,
            TaskHandleKind::WaitForCompletion { cancel } => {
                if let Some(cancel) = cancel {
                    cancel.request_cancel();
                }
                Some(self)
            }
        }
    }
}

struct ProcessController {
    sender: ProcessControlSender,
    stdin_open: bool,
}

struct SpawnProcessControl {
    control_rx: Option<ProcessControlReceiver>,
    process_controller: Option<ProcessController>,
}

impl ProcessController {
    #[must_use]
    fn new(sender: ProcessControlSender) -> Self {
        Self {
            sender,
            stdin_open: true,
        }
    }

    fn write(&mut self, lease_id: u64, bytes: Vec<u8>) -> Result<(), ShellError> {
        if !self.stdin_open {
            return Err(invalid_process_control(lease_id, "stdin is already closed"));
        }

        self.sender
            .try_send(ProcessControlMessage::Write(bytes))
            .map_err(|err| invalid_process_send_error(lease_id, err.to_string()))
    }

    fn close_stdin(&mut self, lease_id: u64) -> Result<(), ShellError> {
        if !self.stdin_open {
            return Err(invalid_process_control(lease_id, "stdin is already closed"));
        }

        self.sender
            .try_send(ProcessControlMessage::CloseStdin)
            .map_err(|err| invalid_process_send_error(lease_id, err.to_string()))?;
        self.stdin_open = false;
        Ok(())
    }
}

struct ActiveTaskEntry {
    owner: TaskLeaseWeak,
    token: u64,
    handle: TaskHandle,
    process_controller: Option<ProcessController>,
    is_process_task: bool,
}

type ActiveTasks = Rc<RefCell<HashMap<u64, ActiveTaskEntry>>>;
type UntrackedTasks = Rc<RefCell<Vec<TaskHandle>>>;
type DeferredEvents<E> = Rc<RefCell<VecDeque<E>>>;
type PendingErrors = Rc<RefCell<VecDeque<ShellError>>>;
type ClosedFlag = Rc<Cell<bool>>;
type ProgressEpoch = Rc<Cell<u64>>;
type LastTermination = Rc<RefCell<ShellDiagnosticsState>>;
type ActiveSubscriptions<E, X> =
    Rc<RefCell<HashMap<SubscriptionKey, ActiveSubscriptionEntry<E, X>>>>;

struct CommandRouter<E, X, Resources>
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    event_tx: EventSender<E>,
    effect_handler: EffectHandlerFn<E, X, Resources>,
    runtime: crate::runtime::Runtime,
    resources: Rc<Resources>,
    activity: Activity,
    tasks: TaskRegistry,
    deferred_events: DeferredEvents<E>,
    pending_errors: PendingErrors,
    deferred_event_overflow_policy: Rc<Cell<DeferredEventOverflowPolicy>>,
    deferred_event_overflow_count: Rc<Cell<usize>>,
    closed: ClosedFlag,
    progress_epoch: ProgressEpoch,
    last_termination: LastTermination,
}

#[derive(Clone)]
struct TaskRegistry {
    active_tasks: ActiveTasks,
    untracked_tasks: UntrackedTasks,
    last_termination: LastTermination,
}

impl TaskRegistry {
    fn new(
        active_tasks: ActiveTasks,
        untracked_tasks: UntrackedTasks,
        last_termination: LastTermination,
    ) -> Self {
        Self {
            active_tasks,
            untracked_tasks,
            last_termination,
        }
    }

    fn push_untracked(&self, handle: TaskHandle) {
        self.untracked_tasks.borrow_mut().push(handle);
    }

    fn cancel_active(&self, lease_id: u64, reason: ShellCancellationReason) {
        cancel_active_task(
            &self.active_tasks,
            &self.untracked_tasks,
            &self.last_termination,
            lease_id,
            reason,
        );
    }

    fn insert_or_track_spawned(&self, spawned: SpawnedTask) {
        if let (Some(lease_id), Some(owner), Some(token)) =
            (spawned.lease_id, spawned.owner, spawned.token)
        {
            self.active_tasks.borrow_mut().insert(
                lease_id,
                ActiveTaskEntry {
                    owner,
                    token,
                    handle: spawned.handle,
                    process_controller: spawned.process_controller,
                    is_process_task: spawned.is_process_task,
                },
            );
        } else {
            self.untracked_tasks.borrow_mut().push(spawned.handle);
        }
    }

    fn write_process(&self, lease: TaskLease, bytes: Vec<u8>) -> Result<(), ShellError> {
        write_to_active_process(&self.active_tasks, &self.last_termination, lease, bytes)
    }

    fn close_process_stdin(&self, lease: TaskLease) -> Result<(), ShellError> {
        close_active_process_stdin(&self.active_tasks, &self.last_termination, lease)
    }

    fn active_tasks(&self) -> &ActiveTasks {
        &self.active_tasks
    }

    fn untracked_tasks(&self) -> &UntrackedTasks {
        &self.untracked_tasks
    }
}

#[derive(Clone)]
struct ProcessSupervisor<E, X, Resources>
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    router: CommandRouter<E, X, Resources>,
    lease_id: Option<u64>,
    token: Option<u64>,
}

#[derive(Clone)]
struct SubscriptionRegistry<E, X>
where
    E: 'static,
    X: 'static,
{
    active_subscriptions: ActiveSubscriptions<E, X>,
    subscription_drivers: Rc<SubscriptionDrivers>,
    untracked_tasks: UntrackedTasks,
    last_termination: LastTermination,
}

impl<E, X> SubscriptionRegistry<E, X>
where
    E: 'static,
    X: 'static,
{
    fn new(
        active_subscriptions: ActiveSubscriptions<E, X>,
        subscription_drivers: Rc<SubscriptionDrivers>,
        untracked_tasks: UntrackedTasks,
        last_termination: LastTermination,
    ) -> Self {
        Self {
            active_subscriptions,
            subscription_drivers,
            untracked_tasks,
            last_termination,
        }
    }

    fn reconcile<Resources: 'static>(
        &self,
        desired: Subscription<E, X>,
        router: &CommandRouter<E, X, Resources>,
    ) -> Result<usize, ShellError> {
        let mut desired_by_key = HashMap::new();
        for entry in desired.into_entries() {
            let key = entry.key().clone();
            if desired_by_key.insert(key.clone(), entry).is_some() {
                return Err(ShellError::DuplicateSubscriptionKey(format!("{key:?}")));
            }
        }

        let active_keys = self
            .active_subscriptions
            .borrow()
            .keys()
            .cloned()
            .collect::<Vec<_>>();

        let mut changes = 0usize;
        for key in active_keys {
            let Some(entry) = desired_by_key.remove(&key) else {
                cancel_active_subscription(
                    &self.active_subscriptions,
                    &self.untracked_tasks,
                    &self.last_termination,
                    &key,
                    ShellCancellationReason::SubscriptionReconciled,
                );
                changes = changes.saturating_add(1);
                continue;
            };

            if active_subscription_matches(
                &self.active_subscriptions,
                &key,
                entry.driver_id(),
                entry.spec(),
            ) {
                let (_, _, _, _, mapper) = entry.into_parts();
                replace_active_subscription_mapper(&self.active_subscriptions, &key, mapper);
                continue;
            }

            cancel_active_subscription(
                &self.active_subscriptions,
                &self.untracked_tasks,
                &self.last_termination,
                &key,
                ShellCancellationReason::SubscriptionReconciled,
            );
            self.start(entry, router)?;
            changes = changes.saturating_add(1);
        }

        for entry in desired_by_key.into_values() {
            self.start(entry, router)?;
            changes = changes.saturating_add(1);
        }

        record_trace(
            &self.last_termination,
            ShellTraceEvent::SubscriptionsReconciled { changes },
        );

        Ok(changes)
    }

    fn start<Resources: 'static>(
        &self,
        entry: SubscriptionEntry<E, X>,
        router: &CommandRouter<E, X, Resources>,
    ) -> Result<(), ShellError> {
        let (key, driver_id, driver_name, spec, mapper) = entry.into_parts();
        if !self.subscription_drivers.contains(driver_id) {
            return Err(ShellError::MissingSubscriptionDriver(driver_name.into()));
        }

        let mapper = Rc::new(RefCell::new(mapper));
        let token = next_task_token();
        record_trace(
            &self.last_termination,
            ShellTraceEvent::SubscriptionStarted {
                key: format!("{key:?}"),
                driver: driver_name,
            },
        );
        let handle = spawn_subscription_runtime_task(
            key.clone(),
            Rc::clone(&mapper),
            token,
            Rc::clone(&self.subscription_drivers),
            driver_id,
            driver_name,
            spec.clone(),
            router,
            &self.active_subscriptions,
        );

        self.active_subscriptions.borrow_mut().insert(
            key,
            ActiveSubscriptionEntry {
                driver_id,
                spec,
                mapper,
                token,
                handle,
            },
        );
        Ok(())
    }

    fn drain_for_shutdown(&self) -> bool {
        drain_active_subscriptions_for_shutdown(
            &self.active_subscriptions,
            &self.untracked_tasks,
            &self.last_termination,
        )
    }
}

impl<E, X, Resources> ProcessSupervisor<E, X, Resources>
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    fn new(
        router: &CommandRouter<E, X, Resources>,
        lease_id: Option<u64>,
        token: Option<u64>,
    ) -> Self {
        Self {
            router: router.clone(),
            lease_id,
            token,
        }
    }

    fn route_update(
        &self,
        update: ProcessUpdate,
        on_update: &mut dyn FnMut(ProcessUpdate) -> Option<Command<E, X>>,
    ) {
        if let ProcessUpdate::Exited(result) = &update {
            if let (Some(lease_id), Some(token)) = (self.lease_id, self.token) {
                let reason = match result {
                    Ok(_) => ShellTerminationReason::Completed,
                    Err(_) => ShellTerminationReason::Failed,
                };
                cleanup_active_process_if_current(
                    self.router.tasks.active_tasks(),
                    &self.router.last_termination,
                    lease_id,
                    token,
                    reason,
                );
            }
        }

        if let Some(command) = on_update(update) {
            self.router.route_spawned_command(command);
        }
    }
}

impl<E, X, Resources> Clone for CommandRouter<E, X, Resources>
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    fn clone(&self) -> Self {
        Self {
            event_tx: self.event_tx.clone(),
            effect_handler: Rc::clone(&self.effect_handler),
            runtime: self.runtime.clone(),
            resources: Rc::clone(&self.resources),
            activity: self.activity.clone(),
            tasks: self.tasks.clone(),
            deferred_events: Rc::clone(&self.deferred_events),
            pending_errors: Rc::clone(&self.pending_errors),
            deferred_event_overflow_policy: Rc::clone(&self.deferred_event_overflow_policy),
            deferred_event_overflow_count: Rc::clone(&self.deferred_event_overflow_count),
            closed: Rc::clone(&self.closed),
            progress_epoch: Rc::clone(&self.progress_epoch),
            last_termination: Rc::clone(&self.last_termination),
        }
    }
}

struct SpawnedTask {
    lease_id: Option<u64>,
    owner: Option<TaskLeaseWeak>,
    token: Option<u64>,
    handle: TaskHandle,
    process_controller: Option<ProcessController>,
    is_process_task: bool,
}

struct ActiveSubscriptionEntry<E, X>
where
    E: 'static,
    X: 'static,
{
    driver_id: TypeId,
    spec: SubscriptionSpec,
    mapper: Rc<RefCell<Box<dyn ErasedSubscriptionMapper<E, X>>>>,
    token: u64,
    handle: TaskHandle,
}

struct TaskLifecycleGuard {
    activity: Activity,
    active_tasks: ActiveTasks,
    last_termination: LastTermination,
    lease_id: Option<u64>,
    token: Option<u64>,
}

impl TaskLifecycleGuard {
    fn new(
        activity: Activity,
        active_tasks: ActiveTasks,
        last_termination: LastTermination,
        lease_id: Option<u64>,
        token: Option<u64>,
    ) -> Self {
        Self {
            activity,
            active_tasks,
            last_termination,
            lease_id,
            token,
        }
    }
}

impl Drop for TaskLifecycleGuard {
    fn drop(&mut self) {
        self.activity.dec();
        if let (Some(lease_id), Some(token)) = (self.lease_id, self.token) {
            cleanup_active_task_if_current(
                &self.active_tasks,
                &self.last_termination,
                lease_id,
                token,
            );
        }
    }
}

struct SubscriptionLifecycleGuard<E, X>
where
    E: 'static,
    X: 'static,
{
    activity: Activity,
    active_subscriptions: ActiveSubscriptions<E, X>,
    last_termination: LastTermination,
    key: SubscriptionKey,
    token: u64,
}

impl<E, X> SubscriptionLifecycleGuard<E, X>
where
    E: 'static,
    X: 'static,
{
    fn new(
        activity: Activity,
        active_subscriptions: ActiveSubscriptions<E, X>,
        last_termination: LastTermination,
        key: SubscriptionKey,
        token: u64,
    ) -> Self {
        Self {
            activity,
            active_subscriptions,
            last_termination,
            key,
            token,
        }
    }
}

impl<E, X> Drop for SubscriptionLifecycleGuard<E, X>
where
    E: 'static,
    X: 'static,
{
    fn drop(&mut self) {
        self.activity.dec();
        cleanup_active_subscription_if_current(
            &self.active_subscriptions,
            &self.last_termination,
            &self.key,
            self.token,
        );
    }
}

static NEXT_TASK_TOKEN: AtomicU64 = AtomicU64::new(1);

fn next_task_token() -> u64 {
    NEXT_TASK_TOKEN.fetch_add(1, Ordering::Relaxed)
}

fn mark_progress(progress_epoch: &ProgressEpoch) {
    let next = progress_epoch.get().wrapping_add(1);
    progress_epoch.set(next);
}

const MAX_DEFERRED_EVENTS: usize = 65_536;
const MAX_INLINE_STREAM_COMMANDS: usize = 10_000;
const DEFERRED_STREAM_BACKPRESSURE_THRESHOLD: usize = 256;
const DEFERRED_STREAM_BACKPRESSURE_SLEEP: Duration = Duration::from_millis(1);
const EXECUTOR_POLL_SLICE: Duration = Duration::from_millis(5);

struct PendingWrite {
    bytes: Vec<u8>,
    written: usize,
}

impl PendingWrite {
    #[must_use]
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, written: 0 }
    }

    #[must_use]
    fn remaining(&self) -> &[u8] {
        &self.bytes[self.written..]
    }

    fn advance(&mut self, written: usize) {
        self.written = self.written.saturating_add(written);
    }

    #[must_use]
    fn is_complete(&self) -> bool {
        self.written >= self.bytes.len()
    }
}

enum ProcessCancelReason {
    User,
    Error(ProcessError),
}

enum ProcessEvent {
    CancelRequested,
    ExitResolved(Result<ExitStatus, ProcessError>),
    GraceElapsed,
    Control(Option<ProcessControlMessage>),
    WriteResolved(Result<usize, ProcessError>),
    Stdout(Result<Option<ProcessFrame>, ProcessError>),
    Stderr(Result<Option<ProcessFrame>, ProcessError>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellTerminationTarget {
    Task,
    Process,
    Subscription,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellCancellationReason {
    ExplicitCommand,
    Replacement,
    OwnerDropped,
    SubscriptionReconciled,
    Shutdown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellTerminationReason {
    Completed,
    Failed,
    Cancelled(ShellCancellationReason),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellTerminationRecord {
    pub target: ShellTerminationTarget,
    pub reason: ShellTerminationReason,
    pub lease_id: Option<u64>,
    pub subscription_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellTaskSnapshot {
    pub lease_id: u64,
    pub owner_alive: bool,
    pub is_process_task: bool,
    pub has_process_stdin_control: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellSubscriptionSnapshot {
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellSnapshot {
    pub active_tasks: Vec<ShellTaskSnapshot>,
    pub active_subscriptions: Vec<ShellSubscriptionSnapshot>,
    pub in_flight_count: usize,
    pub untracked_task_count: usize,
    pub deferred_event_count: usize,
    pub queued_command_count: usize,
    pub pending_error_count: usize,
    pub deferred_event_overflow_count: usize,
    pub last_termination: Option<ShellTerminationRecord>,
}

const DEFAULT_TRACE_MAX_ENTRIES: usize = 4_096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellTraceConfig {
    pub enabled: bool,
    pub max_entries: usize,
}

impl Default for ShellTraceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_entries: DEFAULT_TRACE_MAX_ENTRIES,
        }
    }
}

impl ShellTraceConfig {
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    #[must_use]
    pub fn max_entries(mut self, max_entries: usize) -> Self {
        assert!(
            max_entries > 0,
            "ShellTraceConfig::max_entries must be greater than zero"
        );
        self.max_entries = max_entries;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellTraceTaskKind {
    Future,
    Stream,
    Process,
    Blocking,
    BlockingCooperative,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellTraceCommandStep {
    Event,
    Effect {
        effect_type: &'static str,
    },
    Abortable {
        lease_id: u64,
        effect_type: &'static str,
    },
    Cancel {
        lease_id: u64,
    },
    ProcessWrite {
        lease_id: u64,
        bytes: usize,
    },
    ProcessCloseStdin {
        lease_id: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellTraceProcessControl {
    Write { bytes: usize },
    CloseStdin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellTraceEvent {
    BootCommandDispatched,
    CoreEventCommandDispatched,
    CoreEventsProcessed {
        count: usize,
    },
    SubscriptionsReconciled {
        changes: usize,
    },
    ShellDrained {
        made_progress: bool,
        deferred_events: usize,
        in_flight: usize,
    },
    DeferredEventOverflow {
        limit: usize,
        dropped_events: usize,
        policy: DeferredEventOverflowPolicy,
    },
    CommandStep(ShellTraceCommandStep),
    TaskSpawned {
        lease_id: Option<u64>,
        kind: ShellTraceTaskKind,
    },
    SubscriptionStarted {
        key: String,
        driver: &'static str,
    },
    SubscriptionUpdated {
        key: String,
    },
    ProcessControl {
        lease_id: u64,
        action: ShellTraceProcessControl,
    },
    Termination(ShellTerminationRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellTraceEntry {
    pub sequence: u64,
    pub event: ShellTraceEvent,
}

#[derive(Default)]
struct TraceRecorder {
    config: ShellTraceConfig,
    next_sequence: u64,
    entries: VecDeque<ShellTraceEntry>,
}

impl TraceRecorder {
    #[must_use]
    fn with_config(config: ShellTraceConfig) -> Self {
        Self {
            config,
            next_sequence: 0,
            entries: VecDeque::with_capacity(config.max_entries),
        }
    }

    #[must_use]
    fn config(&self) -> ShellTraceConfig {
        self.config
    }

    fn set_config(&mut self, config: ShellTraceConfig) {
        assert!(
            config.max_entries > 0,
            "ShellTraceConfig::max_entries must be greater than zero"
        );
        self.config = config;
        while self.entries.len() > self.config.max_entries {
            self.entries.pop_front();
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn record(&mut self, event: ShellTraceEvent) {
        if !self.config.enabled {
            return;
        }

        if self.entries.len() >= self.config.max_entries {
            self.entries.pop_front();
        }

        let entry = ShellTraceEntry {
            sequence: self.next_sequence,
            event,
        };
        self.next_sequence = self.next_sequence.wrapping_add(1);
        self.entries.push_back(entry);
    }

    #[must_use]
    fn snapshot(&self) -> Vec<ShellTraceEntry> {
        self.entries.iter().cloned().collect()
    }

    fn take(&mut self) -> Vec<ShellTraceEntry> {
        self.entries.drain(..).collect()
    }
}

struct ShellDiagnosticsState {
    last_termination: Option<ShellTerminationRecord>,
    trace: TraceRecorder,
}

#[derive(Clone)]
struct ShellDiagnostics {
    state: LastTermination,
}

impl ShellDiagnostics {
    fn new(state: LastTermination) -> Self {
        Self { state }
    }

    fn trace_config(&self) -> ShellTraceConfig {
        self.state.borrow().trace.config()
    }

    fn set_trace_config(&self, config: ShellTraceConfig) {
        self.state.borrow_mut().trace.set_config(config);
    }

    fn trace_snapshot(&self) -> Vec<ShellTraceEntry> {
        self.state.borrow().trace.snapshot()
    }

    fn take_trace(&self) -> Vec<ShellTraceEntry> {
        self.state.borrow_mut().trace.take()
    }

    fn clear_trace(&self) {
        self.state.borrow_mut().trace.clear();
    }

    fn record_trace(&self, event: ShellTraceEvent) {
        record_trace(&self.state, event);
    }

    fn last_termination(&self) -> Option<ShellTerminationRecord> {
        self.state.borrow().last_termination.clone()
    }
}

pub struct Shell<E, X, M = (), Resources = ()>
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    event_tx: EventSender<E>,
    effect_handler: EffectHandlerFn<E, X, Resources>,
    boot_handler: Option<BootHandlerFn<E, X, M>>,
    subscription_handler: Option<SubscriptionHandlerFn<E, X, M>>,
    runtime: crate::runtime::Runtime,
    resources: Rc<Resources>,
    activity: Activity,
    active_tasks: ActiveTasks,
    active_subscriptions: ActiveSubscriptions<E, X>,
    subscriptions: SubscriptionRegistry<E, X>,
    untracked_tasks: UntrackedTasks,
    deferred_events: DeferredEvents<E>,
    pending_errors: PendingErrors,
    closed: ClosedFlag,
    progress_epoch: ProgressEpoch,
    last_termination: LastTermination,
    diagnostics: ShellDiagnostics,
    unhandled_effects_policy: Rc<Cell<UnhandledEffectPolicy>>,
    deferred_event_overflow_policy: Rc<Cell<DeferredEventOverflowPolicy>>,
    deferred_event_overflow_count: Rc<Cell<usize>>,
    queue: VecDeque<Command<E, X>>,
}

impl<E, X> Shell<E, X, (), ()>
where
    E: 'static,
    X: 'static,
{
    pub fn new(
        event_tx: EventSender<E>,
        effect_handler: EffectHandlerFn<E, X, ()>,
        runtime: crate::runtime::Runtime,
    ) -> Self {
        Self::with_subscriptions(
            event_tx,
            effect_handler,
            LifecycleHandlers::none(),
            SubscriptionDrivers::new(),
            (),
            runtime,
            Rc::new(Cell::new(UnhandledEffectPolicy::Error)),
            Rc::new(Cell::new(DeferredEventOverflowPolicy::Error)),
        )
    }
}

impl<E, X, M, Resources> Shell<E, X, M, Resources>
where
    E: 'static,
    X: 'static,
    M: 'static,
    Resources: 'static,
{
    fn command_router(&self) -> CommandRouter<E, X, Resources> {
        CommandRouter {
            event_tx: self.event_tx.clone(),
            effect_handler: Rc::clone(&self.effect_handler),
            runtime: self.runtime.clone(),
            resources: Rc::clone(&self.resources),
            activity: self.activity.clone(),
            tasks: TaskRegistry::new(
                Rc::clone(&self.active_tasks),
                Rc::clone(&self.untracked_tasks),
                Rc::clone(&self.last_termination),
            ),
            deferred_events: Rc::clone(&self.deferred_events),
            pending_errors: Rc::clone(&self.pending_errors),
            deferred_event_overflow_policy: Rc::clone(&self.deferred_event_overflow_policy),
            deferred_event_overflow_count: Rc::clone(&self.deferred_event_overflow_count),
            closed: Rc::clone(&self.closed),
            progress_epoch: Rc::clone(&self.progress_epoch),
            last_termination: Rc::clone(&self.last_termination),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_subscriptions(
        event_tx: EventSender<E>,
        effect_handler: EffectHandlerFn<E, X, Resources>,
        lifecycle_handlers: LifecycleHandlers<E, X, M>,
        subscription_drivers: SubscriptionDrivers,
        resources: Resources,
        runtime: crate::runtime::Runtime,
        unhandled_effects_policy: Rc<Cell<UnhandledEffectPolicy>>,
        deferred_event_overflow_policy: Rc<Cell<DeferredEventOverflowPolicy>>,
    ) -> Self {
        let subscription_drivers = Rc::new(subscription_drivers);
        let active_subscriptions = Rc::new(RefCell::new(HashMap::new()));
        let untracked_tasks = Rc::new(RefCell::new(Vec::new()));
        let last_termination = Rc::new(RefCell::new(ShellDiagnosticsState {
            last_termination: None,
            trace: TraceRecorder::with_config(ShellTraceConfig::default()),
        }));
        let subscriptions = SubscriptionRegistry::new(
            Rc::clone(&active_subscriptions),
            Rc::clone(&subscription_drivers),
            Rc::clone(&untracked_tasks),
            Rc::clone(&last_termination),
        );

        Self {
            event_tx,
            effect_handler,
            boot_handler: lifecycle_handlers.boot_handler,
            subscription_handler: lifecycle_handlers.subscription_handler,
            runtime,
            resources: Rc::new(resources),
            activity: Activity::new(),
            active_tasks: Rc::new(RefCell::new(HashMap::new())),
            active_subscriptions,
            subscriptions,
            untracked_tasks,
            deferred_events: Rc::new(RefCell::new(VecDeque::new())),
            pending_errors: Rc::new(RefCell::new(VecDeque::new())),
            closed: Rc::new(Cell::new(false)),
            progress_epoch: Rc::new(Cell::new(0)),
            last_termination: Rc::clone(&last_termination),
            diagnostics: ShellDiagnostics::new(last_termination),
            unhandled_effects_policy,
            deferred_event_overflow_policy,
            deferred_event_overflow_count: Rc::new(Cell::new(0)),
            queue: VecDeque::new(),
        }
    }

    pub fn dispatch_command(&mut self, command: Command<E, X>) -> Result<(), ShellError> {
        if self.closed.get() {
            return Ok(());
        }
        if let Some(err) = take_pending_error(&self.pending_errors) {
            return Err(err);
        }

        prune_orphaned_active_tasks(
            &self.active_tasks,
            &self.untracked_tasks,
            &self.last_termination,
        );
        self.queue.clear();
        self.queue.push_back(command);

        let router = self.command_router();

        router.route_command_iterative(&mut self.queue)?;

        if let Some(err) = take_pending_error(&self.pending_errors) {
            return Err(err);
        }

        Ok(())
    }

    pub fn drain(&mut self) -> Result<usize, ShellError> {
        if let Some(err) = take_pending_error(&self.pending_errors) {
            return Err(err);
        }

        prune_orphaned_active_tasks(
            &self.active_tasks,
            &self.untracked_tasks,
            &self.last_termination,
        );
        prune_finished_untracked_tasks(&self.untracked_tasks);

        let progress_before = self.progress_epoch.get();
        let activity_before = self.activity.load();
        let deferred_before = self.deferred_events.borrow().len();

        let mut progressed = flush_deferred_events(&self.event_tx, &self.deferred_events);
        if activity_before > 0 {
            self.runtime.drive_ready();
        }
        progressed =
            progressed.saturating_add(flush_deferred_events(&self.event_tx, &self.deferred_events));

        prune_orphaned_active_tasks(
            &self.active_tasks,
            &self.untracked_tasks,
            &self.last_termination,
        );
        prune_finished_untracked_tasks(&self.untracked_tasks);

        let activity_after = self.activity.load();
        let deferred_after = self.deferred_events.borrow().len();
        let made_progress = progressed > 0
            || self.progress_epoch.get() != progress_before
            || activity_after != activity_before
            || deferred_after != deferred_before;

        if let Some(err) = take_pending_error(&self.pending_errors) {
            return Err(err);
        }

        record_trace(
            &self.last_termination,
            ShellTraceEvent::ShellDrained {
                made_progress,
                deferred_events: deferred_after,
                in_flight: activity_after,
            },
        );

        Ok(usize::from(made_progress))
    }

    #[must_use]
    /// Returns true only when the shell is quiescent.
    ///
    /// Quiescent means no runtime activity, no deferred/queued routable work,
    /// and no pending shell errors.
    pub fn is_idle(&self) -> bool {
        self.is_quiescent()
    }

    #[must_use]
    /// Returns true when runtime-owned tasks/subscriptions are currently in flight.
    pub fn has_runtime_activity(&self) -> bool {
        self.activity.load() > 0
    }

    #[must_use]
    /// Returns true when shell-routable work remains (queued commands or deferred events).
    pub fn has_routable_work(&self) -> bool {
        !self.queue.is_empty() || !self.deferred_events.borrow().is_empty()
    }

    #[must_use]
    /// Returns true when shell errors have been recorded but not yet surfaced.
    pub fn has_pending_errors(&self) -> bool {
        !self.pending_errors.borrow().is_empty()
    }

    #[must_use]
    /// Returns true when the shell has no runtime activity, no routable work,
    /// and no pending errors.
    pub fn is_quiescent(&self) -> bool {
        !self.has_runtime_activity() && !self.has_routable_work() && !self.has_pending_errors()
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }

    #[must_use]
    pub fn runtime(&self) -> &crate::runtime::Runtime {
        &self.runtime
    }

    #[must_use]
    pub fn has_pending_boot(&self) -> bool {
        self.boot_handler.is_some()
    }

    #[must_use]
    pub fn unhandled_effects_policy(&self) -> UnhandledEffectPolicy {
        self.unhandled_effects_policy.get()
    }

    pub fn set_unhandled_effects_policy(&mut self, policy: UnhandledEffectPolicy) {
        self.unhandled_effects_policy.set(policy);
    }

    #[must_use]
    pub fn deferred_event_overflow_policy(&self) -> DeferredEventOverflowPolicy {
        self.deferred_event_overflow_policy.get()
    }

    pub fn set_deferred_event_overflow_policy(&mut self, policy: DeferredEventOverflowPolicy) {
        self.deferred_event_overflow_policy.set(policy);
    }

    #[must_use]
    pub fn trace_config(&self) -> ShellTraceConfig {
        self.diagnostics.trace_config()
    }

    pub fn set_trace_config(&mut self, config: ShellTraceConfig) {
        self.diagnostics.set_trace_config(config);
    }

    #[must_use]
    pub fn trace_snapshot(&self) -> Vec<ShellTraceEntry> {
        self.diagnostics.trace_snapshot()
    }

    pub fn take_trace(&mut self) -> Vec<ShellTraceEntry> {
        self.diagnostics.take_trace()
    }

    pub fn clear_trace(&mut self) {
        self.diagnostics.clear_trace();
    }

    pub(crate) fn record_trace_event(&self, event: ShellTraceEvent) {
        self.diagnostics.record_trace(event);
    }

    #[must_use]
    pub fn snapshot(&self) -> ShellSnapshot {
        let mut active_tasks = self
            .active_tasks
            .borrow()
            .iter()
            .map(|(&lease_id, entry)| ShellTaskSnapshot {
                lease_id,
                owner_alive: entry.owner.has_owner(),
                is_process_task: entry.is_process_task,
                has_process_stdin_control: entry.process_controller.is_some(),
            })
            .collect::<Vec<_>>();
        active_tasks.sort_by_key(|task| task.lease_id);

        let mut active_subscriptions = self
            .active_subscriptions
            .borrow()
            .keys()
            .map(|key| ShellSubscriptionSnapshot {
                key: format!("{key:?}"),
            })
            .collect::<Vec<_>>();
        active_subscriptions.sort_by(|left, right| left.key.cmp(&right.key));

        ShellSnapshot {
            active_tasks,
            active_subscriptions,
            in_flight_count: self.activity.load(),
            untracked_task_count: self.untracked_tasks.borrow().len(),
            deferred_event_count: self.deferred_events.borrow().len(),
            queued_command_count: self.queue.len(),
            pending_error_count: self.pending_errors.borrow().len(),
            deferred_event_overflow_count: self.deferred_event_overflow_count.get(),
            last_termination: self.diagnostics.last_termination(),
        }
    }

    pub(crate) fn reconcile_subscriptions_for_model(
        &mut self,
        model: &M,
    ) -> Result<usize, ShellError> {
        let Some(handler) = self.subscription_handler.as_ref() else {
            return Ok(0);
        };

        let ctx = crate::extract::SubscriptionContext::new(model);
        let desired = handler(&ctx);
        self.reconcile_subscriptions(desired)
    }

    pub(crate) fn take_boot_command_for_model(&mut self, model: &M) -> Option<Command<E, X>> {
        let handler = self.boot_handler.take()?;
        Some(handler(model))
    }

    pub(crate) fn reconcile_subscriptions(
        &mut self,
        desired: Subscription<E, X>,
    ) -> Result<usize, ShellError> {
        if self.closed.get() {
            return Ok(0);
        }
        if let Some(err) = take_pending_error(&self.pending_errors) {
            return Err(err);
        }

        let router = self.command_router();
        let changes = self.subscriptions.reconcile(desired, &router)?;

        if let Some(err) = take_pending_error(&self.pending_errors) {
            return Err(err);
        }

        Ok(changes)
    }

    pub fn shutdown(&mut self) {
        self.closed.set(true);
        self.deferred_events.borrow_mut().clear();
        let had_active_task_shutdown = drain_active_tasks_for_shutdown(
            &self.active_tasks,
            &self.untracked_tasks,
            &self.last_termination,
        );
        let had_subscription_shutdown = self.subscriptions.drain_for_shutdown();
        request_shutdown_for_untracked_tasks(
            &self.untracked_tasks,
            &self.last_termination,
            !(had_active_task_shutdown || had_subscription_shutdown),
        );
    }

    pub fn wait_for_executors(&self) {
        prune_orphaned_active_tasks(
            &self.active_tasks,
            &self.untracked_tasks,
            &self.last_termination,
        );
        prune_finished_untracked_tasks(&self.untracked_tasks);

        if self.activity.load() == 0 {
            return;
        }

        while self.activity.load() > 0 {
            prune_orphaned_active_tasks(
                &self.active_tasks,
                &self.untracked_tasks,
                &self.last_termination,
            );
            prune_finished_untracked_tasks(&self.untracked_tasks);
            self.runtime.park(EXECUTOR_POLL_SLICE);
        }

        prune_orphaned_active_tasks(
            &self.active_tasks,
            &self.untracked_tasks,
            &self.last_termination,
        );
        prune_finished_untracked_tasks(&self.untracked_tasks);
    }

    pub(crate) fn park_runtime(&self, duration: Duration) {
        self.runtime.park(duration);
    }
}

fn keep_task_after_cancel(untracked_tasks: &UntrackedTasks, handle: TaskHandle) {
    if let Some(handle) = handle.request_cancel() {
        untracked_tasks.borrow_mut().push(handle);
    }
}

fn record_trace(last_termination: &LastTermination, event: ShellTraceEvent) {
    last_termination.borrow_mut().trace.record(event);
}

fn record_termination(
    last_termination: &LastTermination,
    target: ShellTerminationTarget,
    reason: ShellTerminationReason,
    lease_id: Option<u64>,
    subscription_key: Option<String>,
) {
    let termination = ShellTerminationRecord {
        target,
        reason,
        lease_id,
        subscription_key,
    };
    let mut diagnostics = last_termination.borrow_mut();
    diagnostics.last_termination = Some(termination.clone());
    diagnostics
        .trace
        .record(ShellTraceEvent::Termination(termination));
}

fn cleanup_active_subscription_if_current<E, X>(
    active_subscriptions: &ActiveSubscriptions<E, X>,
    last_termination: &LastTermination,
    key: &SubscriptionKey,
    token: u64,
) where
    E: 'static,
    X: 'static,
{
    if let std::collections::hash_map::Entry::Occupied(entry) =
        active_subscriptions.borrow_mut().entry(key.clone())
    {
        if entry.get().token == token {
            entry.remove();
            record_termination(
                last_termination,
                ShellTerminationTarget::Subscription,
                ShellTerminationReason::Completed,
                None,
                Some(format!("{key:?}")),
            );
        }
    }
}

fn active_subscription_is_current<E, X>(
    active_subscriptions: &ActiveSubscriptions<E, X>,
    key: &SubscriptionKey,
    token: u64,
) -> bool
where
    E: 'static,
    X: 'static,
{
    active_subscriptions
        .borrow()
        .get(key)
        .is_some_and(|entry| entry.token == token)
}

fn active_subscription_matches<E, X>(
    active_subscriptions: &ActiveSubscriptions<E, X>,
    key: &SubscriptionKey,
    driver_id: TypeId,
    spec: &SubscriptionSpec,
) -> bool
where
    E: 'static,
    X: 'static,
{
    let active_subscriptions = active_subscriptions.borrow();
    let Some(active) = active_subscriptions.get(key) else {
        return false;
    };

    active.driver_id == driver_id && active.spec == *spec
}

fn replace_active_subscription_mapper<E, X>(
    active_subscriptions: &ActiveSubscriptions<E, X>,
    key: &SubscriptionKey,
    mapper: Box<dyn ErasedSubscriptionMapper<E, X>>,
) where
    E: 'static,
    X: 'static,
{
    let mut active_subscriptions = active_subscriptions.borrow_mut();
    let active = active_subscriptions
        .get_mut(key)
        .expect("active subscription mapper replacement requires a live key");
    *active.mapper.borrow_mut() = mapper;
}

fn invalid_process_control(lease_id: u64, message: impl Into<String>) -> ShellError {
    ShellError::InvalidProcessControl(format!("lease {lease_id}: {}", message.into()))
}

fn invalid_process_send_error(lease_id: u64, message: impl Into<String>) -> ShellError {
    invalid_process_control(
        lease_id,
        format!("failed to queue control message: {}", message.into()),
    )
}

fn cancel_active_task(
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    last_termination: &LastTermination,
    lease_id: u64,
    cancellation: ShellCancellationReason,
) {
    let removed_entry = {
        let mut active_tasks = active_tasks.borrow_mut();
        active_tasks.remove(&lease_id)
    };

    if let Some(entry) = removed_entry {
        let target = if entry.is_process_task {
            ShellTerminationTarget::Process
        } else {
            ShellTerminationTarget::Task
        };
        record_termination(
            last_termination,
            target,
            ShellTerminationReason::Cancelled(cancellation),
            Some(lease_id),
            None,
        );
        keep_task_after_cancel(untracked_tasks, entry.handle);
    }
}

fn cancel_active_subscription<E, X>(
    active_subscriptions: &ActiveSubscriptions<E, X>,
    untracked_tasks: &UntrackedTasks,
    last_termination: &LastTermination,
    key: &SubscriptionKey,
    cancellation: ShellCancellationReason,
) where
    E: 'static,
    X: 'static,
{
    let removed_entry = {
        let mut active_subscriptions = active_subscriptions.borrow_mut();
        active_subscriptions.remove(key)
    };

    if let Some(entry) = removed_entry {
        record_termination(
            last_termination,
            ShellTerminationTarget::Subscription,
            ShellTerminationReason::Cancelled(cancellation),
            None,
            Some(format!("{key:?}")),
        );
        keep_task_after_cancel(untracked_tasks, entry.handle);
    }
}

fn with_process_controller<T>(
    active_tasks: &ActiveTasks,
    lease_id: u64,
    f: impl FnOnce(&mut ProcessController) -> Result<T, ShellError>,
) -> Result<T, ShellError> {
    let mut active_tasks = active_tasks.borrow_mut();
    let entry = active_tasks.get_mut(&lease_id).ok_or_else(|| {
        invalid_process_control(lease_id, "no running process is owned by this lease")
    })?;
    let controller = entry.process_controller.as_mut().ok_or_else(|| {
        invalid_process_control(lease_id, "the running task does not accept stdin control")
    })?;
    f(controller)
}

fn write_to_active_process(
    active_tasks: &ActiveTasks,
    last_termination: &LastTermination,
    lease: TaskLease,
    bytes: Vec<u8>,
) -> Result<(), ShellError> {
    let lease_id = lease.id();
    let byte_count = bytes.len();
    record_trace(
        last_termination,
        ShellTraceEvent::ProcessControl {
            lease_id,
            action: ShellTraceProcessControl::Write { bytes: byte_count },
        },
    );
    with_process_controller(active_tasks, lease_id, |controller| {
        controller.write(lease_id, bytes)
    })
}

fn close_active_process_stdin(
    active_tasks: &ActiveTasks,
    last_termination: &LastTermination,
    lease: TaskLease,
) -> Result<(), ShellError> {
    let lease_id = lease.id();
    record_trace(
        last_termination,
        ShellTraceEvent::ProcessControl {
            lease_id,
            action: ShellTraceProcessControl::CloseStdin,
        },
    );
    with_process_controller(active_tasks, lease_id, |controller| {
        controller.close_stdin(lease_id)
    })
}

fn cleanup_active_task_if_current(
    active_tasks: &ActiveTasks,
    last_termination: &LastTermination,
    lease_id: u64,
    token: u64,
) {
    if let std::collections::hash_map::Entry::Occupied(entry) =
        active_tasks.borrow_mut().entry(lease_id)
    {
        if entry.get().token == token {
            let removed = entry.remove();
            let target = if removed.is_process_task {
                ShellTerminationTarget::Process
            } else {
                ShellTerminationTarget::Task
            };
            record_termination(
                last_termination,
                target,
                ShellTerminationReason::Completed,
                Some(lease_id),
                None,
            );
        }
    }
}

fn cleanup_active_process_if_current(
    active_tasks: &ActiveTasks,
    last_termination: &LastTermination,
    lease_id: u64,
    token: u64,
    reason: ShellTerminationReason,
) {
    if let std::collections::hash_map::Entry::Occupied(entry) =
        active_tasks.borrow_mut().entry(lease_id)
    {
        if entry.get().token == token {
            entry.remove();
            record_termination(
                last_termination,
                ShellTerminationTarget::Process,
                reason,
                Some(lease_id),
                None,
            );
        }
    }
}

fn active_task_is_current(
    active_tasks: &ActiveTasks,
    lease_id: Option<u64>,
    token: Option<u64>,
) -> bool {
    match (lease_id, token) {
        (Some(lease_id), Some(token)) => active_tasks
            .borrow()
            .get(&lease_id)
            .is_some_and(|entry| entry.token == token),
        (None, None) => true,
        _ => false,
    }
}

fn abortable_task_is_alive(
    active_tasks: &ActiveTasks,
    owner: Option<&TaskLeaseWeak>,
    lease_id: Option<u64>,
    token: Option<u64>,
) -> bool {
    let owner_alive = match owner {
        Some(owner) => owner.has_owner(),
        None => true,
    };
    owner_alive && active_task_is_current(active_tasks, lease_id, token)
}

fn prune_orphaned_active_tasks(
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    last_termination: &LastTermination,
) {
    let orphaned = {
        let active_tasks = active_tasks.borrow();
        active_tasks
            .values()
            .filter(|entry| !entry.owner.has_owner())
            .map(|entry| entry.owner.id())
            .collect::<Vec<_>>()
    };

    for lease_id in orphaned {
        cancel_active_task(
            active_tasks,
            untracked_tasks,
            last_termination,
            lease_id,
            ShellCancellationReason::OwnerDropped,
        );
    }
}

fn prune_finished_untracked_tasks(untracked_tasks: &UntrackedTasks) {
    untracked_tasks
        .borrow_mut()
        .retain(|handle| !handle.is_finished());
}

fn take_pending_error(pending_errors: &PendingErrors) -> Option<ShellError> {
    pending_errors.borrow_mut().pop_front()
}

fn drain_active_tasks_for_shutdown(
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    last_termination: &LastTermination,
) -> bool {
    let drained = {
        let mut active_tasks = active_tasks.borrow_mut();
        std::mem::take(&mut *active_tasks)
    };
    let had_entries = !drained.is_empty();

    for (lease_id, entry) in drained {
        let target = if entry.is_process_task {
            ShellTerminationTarget::Process
        } else {
            ShellTerminationTarget::Task
        };
        record_termination(
            last_termination,
            target,
            ShellTerminationReason::Cancelled(ShellCancellationReason::Shutdown),
            Some(lease_id),
            None,
        );
        keep_task_after_cancel(untracked_tasks, entry.handle);
    }

    had_entries
}

fn drain_active_subscriptions_for_shutdown<E, X>(
    active_subscriptions: &ActiveSubscriptions<E, X>,
    untracked_tasks: &UntrackedTasks,
    last_termination: &LastTermination,
) -> bool
where
    E: 'static,
    X: 'static,
{
    let drained = {
        let mut active_subscriptions = active_subscriptions.borrow_mut();
        std::mem::take(&mut *active_subscriptions)
    };
    let had_entries = !drained.is_empty();

    for (key, entry) in drained {
        record_termination(
            last_termination,
            ShellTerminationTarget::Subscription,
            ShellTerminationReason::Cancelled(ShellCancellationReason::Shutdown),
            None,
            Some(format!("{key:?}")),
        );
        keep_task_after_cancel(untracked_tasks, entry.handle);
    }

    had_entries
}

fn request_shutdown_for_untracked_tasks(
    untracked_tasks: &UntrackedTasks,
    last_termination: &LastTermination,
    record_shutdown_termination: bool,
) {
    let drained = {
        let mut untracked_tasks = untracked_tasks.borrow_mut();
        std::mem::take(&mut *untracked_tasks)
    };

    if !drained.is_empty() && record_shutdown_termination {
        record_termination(
            last_termination,
            ShellTerminationTarget::Task,
            ShellTerminationReason::Cancelled(ShellCancellationReason::Shutdown),
            None,
            None,
        );
    }

    let kept = drained
        .into_iter()
        .filter_map(TaskHandle::request_cancel)
        .collect::<Vec<_>>();
    untracked_tasks.borrow_mut().extend(kept);
}

fn push_deferred_event<E>(
    deferred_events: &DeferredEvents<E>,
    pending_errors: &PendingErrors,
    deferred_event_overflow_policy: &Rc<Cell<DeferredEventOverflowPolicy>>,
    deferred_event_overflow_count: &Rc<Cell<usize>>,
    last_termination: &LastTermination,
    event: E,
) {
    let mut deferred_events = deferred_events.borrow_mut();
    if deferred_events.len() >= MAX_DEFERRED_EVENTS {
        let dropped_events = deferred_event_overflow_count.get().saturating_add(1);
        deferred_event_overflow_count.set(dropped_events);
        let policy = deferred_event_overflow_policy.get();

        record_trace(
            last_termination,
            ShellTraceEvent::DeferredEventOverflow {
                limit: MAX_DEFERRED_EVENTS,
                dropped_events,
                policy,
            },
        );

        match policy {
            DeferredEventOverflowPolicy::Error => {
                pending_errors
                    .borrow_mut()
                    .push_back(ShellError::DeferredEventOverflow {
                        limit: MAX_DEFERRED_EVENTS,
                        dropped_events,
                    });
            }
            DeferredEventOverflowPolicy::DropNewest => {
                report_spawned_event_drop(
                    "dropping deferred event because backlog reached hard limit",
                );
            }
        }
        return;
    }

    deferred_events.push_back(event);
}

impl<E, X, Resources> CommandRouter<E, X, Resources>
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    fn route_command_iterative(
        &self,
        queue: &mut VecDeque<Command<E, X>>,
    ) -> Result<(), ShellError> {
        while let Some(command) = queue.pop_front() {
            if self.closed.get() {
                return Ok(());
            }

            for step in command {
                match step {
                    CommandStep::Event(event) => {
                        record_trace(
                            &self.last_termination,
                            ShellTraceEvent::CommandStep(ShellTraceCommandStep::Event),
                        );
                        self.route_event(event)
                            .map_err(|_| ShellError::EventChannelClosed)?;
                    }
                    CommandStep::Effect(effect) => {
                        record_trace(
                            &self.last_termination,
                            ShellTraceEvent::CommandStep(ShellTraceCommandStep::Effect {
                                effect_type: std::any::type_name::<X>(),
                            }),
                        );
                        if let Some(spawned) = run_effect_task(effect, None, self, queue)? {
                            self.tasks.push_untracked(spawned.handle);
                        }
                    }
                    CommandStep::Abortable { lease, effect } => {
                        record_trace(
                            &self.last_termination,
                            ShellTraceEvent::CommandStep(ShellTraceCommandStep::Abortable {
                                lease_id: lease.id(),
                                effect_type: std::any::type_name::<X>(),
                            }),
                        );
                        let ctx = EffectContext::new(self.resources.as_ref());
                        let task = (self.effect_handler)(effect, &ctx)?;
                        cancel_blocking_abortable_task(&task)?;
                        self.tasks
                            .cancel_active(lease.id(), ShellCancellationReason::Replacement);
                        if let Some(spawned) = spawn_task(task, Some(lease), self, queue)? {
                            self.tasks.insert_or_track_spawned(spawned);
                        }
                    }
                    CommandStep::Cancel { lease } => {
                        record_trace(
                            &self.last_termination,
                            ShellTraceEvent::CommandStep(ShellTraceCommandStep::Cancel {
                                lease_id: lease.id(),
                            }),
                        );
                        self.tasks
                            .cancel_active(lease.id(), ShellCancellationReason::ExplicitCommand);
                    }
                    CommandStep::ProcessWrite { lease, bytes } => {
                        record_trace(
                            &self.last_termination,
                            ShellTraceEvent::CommandStep(ShellTraceCommandStep::ProcessWrite {
                                lease_id: lease.id(),
                                bytes: bytes.len(),
                            }),
                        );
                        self.tasks.write_process(lease, bytes)?;
                    }
                    CommandStep::ProcessCloseStdin { lease } => {
                        record_trace(
                            &self.last_termination,
                            ShellTraceEvent::CommandStep(
                                ShellTraceCommandStep::ProcessCloseStdin {
                                    lease_id: lease.id(),
                                },
                            ),
                        );
                        self.tasks.close_process_stdin(lease)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn route_event(&self, event: E) -> Result<(), crossbeam_channel::TrySendError<E>> {
        if !self.deferred_events.borrow().is_empty() {
            push_deferred_event(
                &self.deferred_events,
                &self.pending_errors,
                &self.deferred_event_overflow_policy,
                &self.deferred_event_overflow_count,
                &self.last_termination,
                event,
            );
            return Ok(());
        }

        match self.event_tx.try_send_owned(event) {
            Ok(()) => Ok(()),
            Err(crossbeam_channel::TrySendError::Full(event)) => {
                push_deferred_event(
                    &self.deferred_events,
                    &self.pending_errors,
                    &self.deferred_event_overflow_policy,
                    &self.deferred_event_overflow_count,
                    &self.last_termination,
                    event,
                );
                Ok(())
            }
            Err(crossbeam_channel::TrySendError::Disconnected(event)) => {
                Err(crossbeam_channel::TrySendError::Disconnected(event))
            }
        }
    }
}

fn run_effect_task<E, X, Resources>(
    effect: X,
    lease: Option<TaskLease>,
    router: &CommandRouter<E, X, Resources>,
    queue: &mut VecDeque<Command<E, X>>,
) -> Result<Option<SpawnedTask>, ShellError>
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    let ctx = EffectContext::new(router.resources.as_ref());
    let task = (router.effect_handler)(effect, &ctx)?;
    spawn_task(task, lease, router, queue)
}

fn promote_next_write(
    active_write: &mut Option<PendingWrite>,
    queued_writes: &mut VecDeque<Vec<u8>>,
) {
    if active_write.is_none() {
        if let Some(bytes) = queued_writes.pop_front() {
            *active_write = Some(PendingWrite::new(bytes));
        }
    }
}

fn apply_pending_stdin_close(
    stdin: &mut Option<async_process::ChildStdin>,
    active_write: Option<&PendingWrite>,
    queued_writes: &VecDeque<Vec<u8>>,
    stdin_close_requested: &mut bool,
) {
    if *stdin_close_requested && active_write.is_none() && queued_writes.is_empty() {
        let _ = stdin.take();
        *stdin_close_requested = false;
    }
}

fn enqueue_process_control(
    message: ProcessControlMessage,
    active_write: &mut Option<PendingWrite>,
    queued_writes: &mut VecDeque<Vec<u8>>,
    stdin_close_requested: &mut bool,
) {
    match message {
        ProcessControlMessage::Write(bytes) => {
            if active_write.is_none() {
                *active_write = Some(PendingWrite::new(bytes));
            } else {
                queued_writes.push_back(bytes);
            }
        }
        ProcessControlMessage::CloseStdin => {
            *stdin_close_requested = true;
        }
    }
}

fn can_finish_process_task(
    exit_status: Option<&Result<ExitStatus, ProcessError>>,
    cancel_reason: Option<&ProcessCancelReason>,
    stdout: &ProcessOutputDriver,
    stderr: &ProcessOutputDriver,
) -> bool {
    match cancel_reason {
        Some(ProcessCancelReason::User | ProcessCancelReason::Error(_)) => exit_status.is_some(),
        None => match exit_status {
            Some(Ok(_)) => stdout.is_finished() && stderr.is_finished(),
            Some(Err(_)) => true,
            None => false,
        },
    }
}

fn build_process_terminal_update(
    exit_status: &Result<ExitStatus, ProcessError>,
    cancel_reason: Option<&ProcessCancelReason>,
    stdout: &mut ProcessOutputDriver,
    stderr: &mut ProcessOutputDriver,
) -> Option<ProcessUpdate> {
    match cancel_reason {
        Some(ProcessCancelReason::User) => None,
        Some(ProcessCancelReason::Error(err)) => Some(ProcessUpdate::Exited(Err(err.clone()))),
        None => match exit_status {
            Ok(status) => Some(ProcessUpdate::Exited(Ok(crate::process::ProcessExit {
                status: *status,
                stdout: stdout.captured_output(),
                stderr: stderr.captured_output(),
            }))),
            Err(err) => Some(ProcessUpdate::Exited(Err(err.clone()))),
        },
    }
}

fn route_subscription_update<E, X, Resources>(
    mapper: &Rc<RefCell<Box<dyn ErasedSubscriptionMapper<E, X>>>>,
    update: Box<dyn std::any::Any>,
    router: &CommandRouter<E, X, Resources>,
) where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    let Some(command) = mapper.borrow_mut().map(update) else {
        return;
    };

    router.route_spawned_command(command);
}

#[allow(clippy::too_many_arguments)]
fn spawn_subscription_runtime_task<E, X, Resources>(
    key: SubscriptionKey,
    mapper: Rc<RefCell<Box<dyn ErasedSubscriptionMapper<E, X>>>>,
    token: u64,
    subscription_drivers: Rc<SubscriptionDrivers>,
    driver_id: TypeId,
    driver_name: &'static str,
    spec: SubscriptionSpec,
    router: &CommandRouter<E, X, Resources>,
    active_subscriptions: &ActiveSubscriptions<E, X>,
) -> TaskHandle
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    let runtime = router.runtime.clone();
    let active_subscriptions = Rc::clone(active_subscriptions);
    let router_for_task = (*router).clone();
    let pending_errors = Rc::clone(&router.pending_errors);
    let closed = Rc::clone(&router.closed);
    let last_termination = Rc::clone(&router.last_termination);
    let mapper_for_task = Rc::clone(&mapper);
    router.activity.inc();
    let lifecycle_guard = SubscriptionLifecycleGuard::new(
        router.activity.clone(),
        Rc::clone(&active_subscriptions),
        Rc::clone(&last_termination),
        key.clone(),
        token,
    );
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let mut cancel_future: LocalBoxFuture<'static, ()> = Box::pin(async move {
        let _ = cancel_rx.await;
    });

    let handle = runtime.spawn(async move {
        let _lifecycle_guard = lifecycle_guard;
        let Some(mut stream) = subscription_drivers.subscribe(driver_id, spec) else {
            pending_errors
                .borrow_mut()
                .push_back(ShellError::MissingSubscriptionDriver(driver_name.into()));
            return;
        };

        loop {
            if closed.get() {
                return;
            }

            let mut events: Vec<LocalBoxFuture<'_, Option<Box<dyn std::any::Any>>>> = Vec::new();
            events.push(Box::pin(async {
                (&mut cancel_future).await;
                None
            }));
            events.push(Box::pin(async { stream.next().await }));

            let (update, _, _) = select_all(events).await;
            let Some(update) = update else {
                return;
            };

            if !active_subscription_is_current(&active_subscriptions, &key, token) {
                return;
            }

            record_trace(
                &last_termination,
                ShellTraceEvent::SubscriptionUpdated {
                    key: format!("{key:?}"),
                },
            );

            route_subscription_update(&mapper_for_task, update, &router_for_task);
        }
    });

    TaskHandle::wait_for_completion(
        handle,
        Some(TaskCancelHandle::Subscription(Some(cancel_tx))),
    )
}

async fn write_pending_process_bytes(
    stdin: &mut async_process::ChildStdin,
    pending_write: &PendingWrite,
    spec: &ProcessSpec,
) -> Result<usize, ProcessError> {
    let written = poll_fn(|cx| Pin::new(&mut *stdin).poll_write(cx, pending_write.remaining()))
        .await
        .map_err(|err| {
            ProcessError::new(
                spec,
                crate::process::ProcessErrorKind::StdinWrite,
                err.to_string(),
            )
        })?;
    if written == 0 {
        return Err(ProcessError::new(
            spec,
            crate::process::ProcessErrorKind::StdinWrite,
            io::Error::from(io::ErrorKind::WriteZero).to_string(),
        ));
    }

    Ok(written)
}

#[allow(clippy::too_many_arguments)]
fn start_process_cancel(
    spec: &ProcessSpec,
    cancel_reason: &mut Option<ProcessCancelReason>,
    next_reason: ProcessCancelReason,
    stdin: &mut Option<async_process::ChildStdin>,
    control_rx: &mut Option<ProcessControlReceiver>,
    active_write: &mut Option<PendingWrite>,
    queued_writes: &mut VecDeque<Vec<u8>>,
    grace_timer: &mut Option<LocalBoxFuture<'static, ()>>,
    child: &mut async_process::Child,
    pending_errors: &PendingErrors,
) {
    if cancel_reason.is_some() {
        return;
    }

    *cancel_reason = Some(next_reason);
    *control_rx = None;
    *active_write = None;
    queued_writes.clear();

    match spec.termination_policy_ref() {
        ProcessTerminationPolicy::Kill => {
            try_kill_process(child, spec, pending_errors);
        }
        ProcessTerminationPolicy::CloseStdinThenKill { grace } => {
            let closed_stdin = stdin.take().is_some();
            if closed_stdin {
                let grace = *grace;
                *grace_timer = Some(Box::pin(async move {
                    crate::runtime::sleep(grace).await;
                }));
            } else {
                try_kill_process(child, spec, pending_errors);
            }
        }
    }
}

fn try_kill_process(
    child: &mut async_process::Child,
    spec: &ProcessSpec,
    pending_errors: &PendingErrors,
) {
    match child.kill() {
        Ok(()) => {}
        Err(err) if err.kind() == io::ErrorKind::InvalidInput => {}
        Err(err) => pending_errors
            .borrow_mut()
            .push_back(ShellError::CommandExecutionFailed(format!(
                "failed to kill process `{}`: {err}",
                spec.program().to_string_lossy()
            ))),
    }
}

fn spawn_process_runtime_task<E, X, Resources>(
    spec: ProcessSpec,
    mut on_update: Box<dyn FnMut(ProcessUpdate) -> Option<Command<E, X>> + 'static>,
    mut child: async_process::Child,
    control: SpawnProcessControl,
    lease_id: Option<u64>,
    owner: Option<TaskLeaseWeak>,
    router: &CommandRouter<E, X, Resources>,
) -> SpawnedTask
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    let runtime = router.runtime.clone();
    let activity = router.activity.clone();
    let active_tasks = Rc::clone(router.tasks.active_tasks());
    let pending_errors = Rc::clone(&router.pending_errors);
    let closed = Rc::clone(&router.closed);
    let router_for_task = router.clone();
    let token = lease_id.map(|_| next_task_token());
    let owner_for_task = owner.clone();
    let process_supervisor = ProcessSupervisor::new(&router_for_task, lease_id, token);
    activity.inc();
    let lifecycle_guard = TaskLifecycleGuard::new(
        activity.clone(),
        Rc::clone(&active_tasks),
        Rc::clone(&router.last_termination),
        lease_id,
        token,
    );

    let mut stdin = child.stdin.take();
    let mut stdout = ProcessOutputDriver::stdout(child.stdout.take(), spec.stdout_mode());
    let mut stderr = ProcessOutputDriver::stderr(child.stderr.take(), spec.stderr_mode());
    let status = child.status();
    let spec_for_status = spec.clone();
    let mut status_future: LocalBoxFuture<'static, Result<ExitStatus, ProcessError>> =
        Box::pin(async move {
            status.await.map_err(|err| {
                ProcessError::new(
                    &spec_for_status,
                    crate::process::ProcessErrorKind::Wait,
                    err.to_string(),
                )
            })
        });
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let mut cancel_future: LocalBoxFuture<'static, ()> = Box::pin(async move {
        let _ = cancel_rx.await;
    });

    let SpawnProcessControl {
        mut control_rx,
        process_controller,
    } = control;

    let handle = runtime.spawn(async move {
        let _lifecycle_guard = lifecycle_guard;
        let mut cancel_reason = None;
        let mut grace_timer: Option<LocalBoxFuture<'static, ()>> = None;
        let mut exit_status = None;
        let mut active_write = None;
        let mut queued_writes = VecDeque::new();
        let mut stdin_close_requested = false;

        loop {
            if closed.get() {
                return;
            }

            promote_next_write(&mut active_write, &mut queued_writes);
            apply_pending_stdin_close(
                &mut stdin,
                active_write.as_ref(),
                &queued_writes,
                &mut stdin_close_requested,
            );

            if can_finish_process_task(
                exit_status.as_ref(),
                cancel_reason.as_ref(),
                &stdout,
                &stderr,
            ) {
                let update = build_process_terminal_update(
                    exit_status
                        .as_ref()
                        .expect("process completion requires an exit status"),
                    cancel_reason.as_ref(),
                    &mut stdout,
                    &mut stderr,
                );
                if let Some(update) = update {
                    process_supervisor.route_update(update, &mut on_update);
                }
                return;
            }

            let mut process_events: Vec<LocalBoxFuture<'_, ProcessEvent>> = Vec::new();
            if cancel_reason.is_none() {
                process_events.push(Box::pin(async {
                    (&mut cancel_future).await;
                    ProcessEvent::CancelRequested
                }));
                if let Some(control_rx) = control_rx.as_mut() {
                    process_events.push(Box::pin(async {
                        ProcessEvent::Control(control_rx.next().await)
                    }));
                }
                if let (Some(stdin), Some(active_write_ref)) =
                    (stdin.as_mut(), active_write.as_ref())
                {
                    process_events.push(Box::pin(async {
                        ProcessEvent::WriteResolved(
                            write_pending_process_bytes(stdin, active_write_ref, &spec).await,
                        )
                    }));
                }
                if stdout.should_poll() {
                    process_events.push(Box::pin(async {
                        ProcessEvent::Stdout(stdout.next_frame(&spec).await)
                    }));
                }
                if stderr.should_poll() {
                    process_events.push(Box::pin(async {
                        ProcessEvent::Stderr(stderr.next_frame(&spec).await)
                    }));
                }
            }
            if let Some(grace_timer_ref) = grace_timer.as_mut() {
                process_events.push(Box::pin(async {
                    grace_timer_ref.as_mut().await;
                    ProcessEvent::GraceElapsed
                }));
            }
            if exit_status.is_none() {
                process_events.push(Box::pin(async {
                    ProcessEvent::ExitResolved((&mut status_future).await)
                }));
            }

            let (event, _, _) = select_all(process_events).await;
            match event {
                ProcessEvent::CancelRequested => {
                    start_process_cancel(
                        &spec,
                        &mut cancel_reason,
                        ProcessCancelReason::User,
                        &mut stdin,
                        &mut control_rx,
                        &mut active_write,
                        &mut queued_writes,
                        &mut grace_timer,
                        &mut child,
                        &pending_errors,
                    );
                }
                ProcessEvent::ExitResolved(result) => {
                    exit_status = Some(result);
                }
                ProcessEvent::GraceElapsed => {
                    grace_timer = None;
                    try_kill_process(&mut child, &spec, &pending_errors);
                }
                ProcessEvent::Control(message) => match message {
                    Some(message) => enqueue_process_control(
                        message,
                        &mut active_write,
                        &mut queued_writes,
                        &mut stdin_close_requested,
                    ),
                    None => {
                        control_rx = None;
                    }
                },
                ProcessEvent::WriteResolved(result) => match result {
                    Ok(written) => {
                        let pending_write = active_write
                            .as_mut()
                            .expect("write branch requires a pending write");
                        pending_write.advance(written);
                        if pending_write.is_complete() {
                            active_write = None;
                        }
                    }
                    Err(err) => {
                        start_process_cancel(
                            &spec,
                            &mut cancel_reason,
                            ProcessCancelReason::Error(err),
                            &mut stdin,
                            &mut control_rx,
                            &mut active_write,
                            &mut queued_writes,
                            &mut grace_timer,
                            &mut child,
                            &pending_errors,
                        );
                    }
                },
                ProcessEvent::Stdout(result) => match result {
                    Ok(Some(frame)) => {
                        if abortable_task_is_alive(
                            &active_tasks,
                            owner_for_task.as_ref(),
                            lease_id,
                            token,
                        ) {
                            process_supervisor
                                .route_update(ProcessUpdate::Stdout(frame), &mut on_update);
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        start_process_cancel(
                            &spec,
                            &mut cancel_reason,
                            ProcessCancelReason::Error(err),
                            &mut stdin,
                            &mut control_rx,
                            &mut active_write,
                            &mut queued_writes,
                            &mut grace_timer,
                            &mut child,
                            &pending_errors,
                        );
                    }
                },
                ProcessEvent::Stderr(result) => match result {
                    Ok(Some(frame)) => {
                        if abortable_task_is_alive(
                            &active_tasks,
                            owner_for_task.as_ref(),
                            lease_id,
                            token,
                        ) {
                            process_supervisor
                                .route_update(ProcessUpdate::Stderr(frame), &mut on_update);
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        start_process_cancel(
                            &spec,
                            &mut cancel_reason,
                            ProcessCancelReason::Error(err),
                            &mut stdin,
                            &mut control_rx,
                            &mut active_write,
                            &mut queued_writes,
                            &mut grace_timer,
                            &mut child,
                            &pending_errors,
                        );
                    }
                },
            }
        }
    });

    SpawnedTask {
        lease_id,
        owner,
        token,
        handle: TaskHandle::wait_for_completion(
            handle,
            Some(TaskCancelHandle::Process(Some(cancel_tx))),
        ),
        process_controller,
        is_process_task: true,
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_task<E, X, Resources>(
    task: Task<E, X>,
    lease: Option<TaskLease>,
    router: &CommandRouter<E, X, Resources>,
    queue: &mut VecDeque<Command<E, X>>,
) -> Result<Option<SpawnedTask>, ShellError>
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    let event_tx = &router.event_tx;
    let effect_handler = &router.effect_handler;
    let runtime = &router.runtime;
    let resources = &router.resources;
    let activity = &router.activity;
    let active_tasks = router.tasks.active_tasks();
    let untracked_tasks = router.tasks.untracked_tasks();
    let deferred_events = &router.deferred_events;
    let pending_errors = &router.pending_errors;
    let deferred_event_overflow_policy = &router.deferred_event_overflow_policy;
    let deferred_event_overflow_count = &router.deferred_event_overflow_count;
    let closed = &router.closed;
    let progress_epoch = &router.progress_epoch;
    let last_termination = &router.last_termination;

    let lease_id = lease.as_ref().map(TaskLease::id);
    let owner = lease.as_ref().map(TaskLease::downgrade);
    drop(lease);

    match task {
        Task::None => Ok(None),
        Task::Resolved(command) => {
            queue.push_back(command);
            Ok(None)
        }
        Task::Future(future) => {
            if owner.as_ref().is_some_and(|owner| !owner.has_owner()) {
                return Ok(None);
            }

            record_trace(
                last_termination,
                ShellTraceEvent::TaskSpawned {
                    lease_id,
                    kind: ShellTraceTaskKind::Future,
                },
            );

            let runtime = runtime.clone();
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = Rc::clone(resources);
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let untracked_tasks = Rc::clone(untracked_tasks);
            let deferred_events = Rc::clone(deferred_events);
            let pending_errors = Rc::clone(pending_errors);
            let deferred_event_overflow_policy = Rc::clone(deferred_event_overflow_policy);
            let deferred_event_overflow_count = Rc::clone(deferred_event_overflow_count);
            let closed = Rc::clone(closed);
            let progress_epoch = Rc::clone(progress_epoch);
            let last_termination = Rc::clone(last_termination);
            let token = lease_id.map(|_| next_task_token());
            let owner_for_task = owner.clone();
            activity.inc();
            let lifecycle_guard = TaskLifecycleGuard::new(
                activity.clone(),
                Rc::clone(&active_tasks),
                Rc::clone(&last_termination),
                lease_id,
                token,
            );

            let router_for_task = CommandRouter {
                event_tx: event_tx.clone(),
                effect_handler: Rc::clone(&effect_handler),
                runtime: runtime.clone(),
                resources: Rc::clone(&resources),
                activity: activity.clone(),
                tasks: TaskRegistry::new(
                    Rc::clone(&active_tasks),
                    Rc::clone(&untracked_tasks),
                    Rc::clone(&last_termination),
                ),
                deferred_events: Rc::clone(&deferred_events),
                pending_errors: Rc::clone(&pending_errors),
                deferred_event_overflow_policy: Rc::clone(&deferred_event_overflow_policy),
                deferred_event_overflow_count: Rc::clone(&deferred_event_overflow_count),
                closed: Rc::clone(&closed),
                progress_epoch: Rc::clone(&progress_epoch),
                last_termination: Rc::clone(&last_termination),
            };
            let handle = runtime.spawn(async move {
                let _lifecycle_guard = lifecycle_guard;
                let command = future.await;
                if closed.get() {
                    return;
                }

                if !abortable_task_is_alive(&active_tasks, owner_for_task.as_ref(), lease_id, token)
                {
                    return;
                }

                router_for_task.route_spawned_command(command);
            });

            Ok(Some(SpawnedTask {
                lease_id,
                owner,
                token,
                handle: TaskHandle::abort_on_drop(handle),
                process_controller: None,
                is_process_task: false,
            }))
        }
        Task::Stream(stream) => {
            if owner.as_ref().is_some_and(|owner| !owner.has_owner()) {
                return Ok(None);
            }

            record_trace(
                last_termination,
                ShellTraceEvent::TaskSpawned {
                    lease_id,
                    kind: ShellTraceTaskKind::Stream,
                },
            );

            let runtime = runtime.clone();
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = Rc::clone(resources);
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let untracked_tasks = Rc::clone(untracked_tasks);
            let deferred_events = Rc::clone(deferred_events);
            let pending_errors = Rc::clone(pending_errors);
            let deferred_event_overflow_policy = Rc::clone(deferred_event_overflow_policy);
            let deferred_event_overflow_count = Rc::clone(deferred_event_overflow_count);
            let closed = Rc::clone(closed);
            let progress_epoch = Rc::clone(progress_epoch);
            let last_termination = Rc::clone(last_termination);
            let token = lease_id.map(|_| next_task_token());
            let owner_for_task = owner.clone();
            activity.inc();
            let lifecycle_guard = TaskLifecycleGuard::new(
                activity.clone(),
                Rc::clone(&active_tasks),
                Rc::clone(&last_termination),
                lease_id,
                token,
            );

            let router_for_task = CommandRouter {
                event_tx: event_tx.clone(),
                effect_handler: Rc::clone(&effect_handler),
                runtime: runtime.clone(),
                resources: Rc::clone(&resources),
                activity: activity.clone(),
                tasks: TaskRegistry::new(
                    Rc::clone(&active_tasks),
                    Rc::clone(&untracked_tasks),
                    Rc::clone(&last_termination),
                ),
                deferred_events: Rc::clone(&deferred_events),
                pending_errors: Rc::clone(&pending_errors),
                deferred_event_overflow_policy: Rc::clone(&deferred_event_overflow_policy),
                deferred_event_overflow_count: Rc::clone(&deferred_event_overflow_count),
                closed: Rc::clone(&closed),
                progress_epoch: Rc::clone(&progress_epoch),
                last_termination: Rc::clone(&last_termination),
            };
            let handle = runtime.spawn(async move {
                let _lifecycle_guard = lifecycle_guard;
                futures::pin_mut!(stream);
                let mut emitted_commands = 0usize;
                while let Some(command) = stream.next().await {
                    if closed.get() {
                        break;
                    }

                    if !abortable_task_is_alive(
                        &active_tasks,
                        owner_for_task.as_ref(),
                        lease_id,
                        token,
                    ) {
                        break;
                    }

                    emitted_commands = emitted_commands.saturating_add(1);
                    if emitted_commands > MAX_INLINE_STREAM_COMMANDS {
                        report_spawned_event_drop(
                            "stream command limit reached; dropping remaining commands",
                        );
                        break;
                    }

                    router_for_task.route_spawned_command(command);

                    if closed.get() {
                        break;
                    }

                    if !abortable_task_is_alive(
                        &active_tasks,
                        owner_for_task.as_ref(),
                        lease_id,
                        token,
                    ) {
                        break;
                    }

                    let backlog = event_tx
                        .len()
                        .saturating_add(deferred_events.borrow().len());
                    if backlog >= DEFERRED_STREAM_BACKPRESSURE_THRESHOLD {
                        crate::runtime::sleep(DEFERRED_STREAM_BACKPRESSURE_SLEEP).await;
                    }
                }
            });

            Ok(Some(SpawnedTask {
                lease_id,
                owner,
                token,
                handle: TaskHandle::abort_on_drop(handle),
                process_controller: None,
                is_process_task: false,
            }))
        }
        Task::Process(task) => {
            if owner.as_ref().is_some_and(|owner| !owner.has_owner()) {
                return Ok(None);
            }

            record_trace(
                last_termination,
                ShellTraceEvent::TaskSpawned {
                    lease_id,
                    kind: ShellTraceTaskKind::Process,
                },
            );

            let (spec, mut on_update) = task.into_parts();
            let child = match crate::process::spawn(&spec) {
                Ok(child) => child,
                Err(err) => {
                    if let Some(command) = on_update(ProcessUpdate::Exited(Err(err))) {
                        queue.push_back(command);
                    }
                    return Ok(None);
                }
            };

            let (control_rx, process_controller) =
                if lease_id.is_some() && spec.stdin_mode() == &ProcessInput::Piped {
                    let (sender, receiver) = control_channel();
                    (Some(receiver), Some(ProcessController::new(sender)))
                } else {
                    (None, None)
                };

            let control = SpawnProcessControl {
                control_rx,
                process_controller,
            };

            Ok(Some(spawn_process_runtime_task(
                spec, on_update, child, control, lease_id, owner, router,
            )))
        }
        Task::Blocking(task) => {
            if owner.is_some() {
                return Err(ShellError::AbortableBlockingTask);
            }

            record_trace(
                last_termination,
                ShellTraceEvent::TaskSpawned {
                    lease_id: None,
                    kind: ShellTraceTaskKind::Blocking,
                },
            );

            let runtime = runtime.clone();
            let future = task.into_future(runtime.clone());
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = Rc::clone(resources);
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let untracked_tasks = Rc::clone(untracked_tasks);
            let deferred_events = Rc::clone(deferred_events);
            let pending_errors = Rc::clone(pending_errors);
            let deferred_event_overflow_policy = Rc::clone(deferred_event_overflow_policy);
            let deferred_event_overflow_count = Rc::clone(deferred_event_overflow_count);
            let closed = Rc::clone(closed);
            let progress_epoch = Rc::clone(progress_epoch);
            let last_termination = Rc::clone(last_termination);
            activity.inc();
            let lifecycle_guard = TaskLifecycleGuard::new(
                activity.clone(),
                Rc::clone(&active_tasks),
                Rc::clone(&last_termination),
                None,
                None,
            );

            let router_for_task = CommandRouter {
                event_tx: event_tx.clone(),
                effect_handler: Rc::clone(&effect_handler),
                runtime: runtime.clone(),
                resources: Rc::clone(&resources),
                activity: activity.clone(),
                tasks: TaskRegistry::new(
                    Rc::clone(&active_tasks),
                    Rc::clone(&untracked_tasks),
                    Rc::clone(&last_termination),
                ),
                deferred_events: Rc::clone(&deferred_events),
                pending_errors: Rc::clone(&pending_errors),
                deferred_event_overflow_policy: Rc::clone(&deferred_event_overflow_policy),
                deferred_event_overflow_count: Rc::clone(&deferred_event_overflow_count),
                closed: Rc::clone(&closed),
                progress_epoch: Rc::clone(&progress_epoch),
                last_termination: Rc::clone(&last_termination),
            };
            let handle = runtime.spawn(async move {
                let _lifecycle_guard = lifecycle_guard;
                let command = future.await;
                if closed.get() {
                    return;
                }

                router_for_task.route_spawned_command(command);
            });

            Ok(Some(SpawnedTask {
                lease_id: None,
                owner: None,
                token: None,
                handle: TaskHandle::wait_for_completion(handle, None),
                process_controller: None,
                is_process_task: false,
            }))
        }
        Task::BlockingCooperative(task) => {
            if owner.as_ref().is_some_and(|owner| !owner.has_owner()) {
                return Ok(None);
            }

            record_trace(
                last_termination,
                ShellTraceEvent::TaskSpawned {
                    lease_id,
                    kind: ShellTraceTaskKind::BlockingCooperative,
                },
            );

            let runtime = runtime.clone();
            let cancel = BlockingCancelToken::new();
            let future = task.into_future(runtime.clone(), cancel.clone());
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = Rc::clone(resources);
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let untracked_tasks = Rc::clone(untracked_tasks);
            let deferred_events = Rc::clone(deferred_events);
            let pending_errors = Rc::clone(pending_errors);
            let deferred_event_overflow_policy = Rc::clone(deferred_event_overflow_policy);
            let deferred_event_overflow_count = Rc::clone(deferred_event_overflow_count);
            let closed = Rc::clone(closed);
            let progress_epoch = Rc::clone(progress_epoch);
            let last_termination = Rc::clone(last_termination);
            let token = lease_id.map(|_| next_task_token());
            let owner_for_task = owner.clone();
            activity.inc();
            let lifecycle_guard = TaskLifecycleGuard::new(
                activity.clone(),
                Rc::clone(&active_tasks),
                Rc::clone(&last_termination),
                lease_id,
                token,
            );

            let router_for_task = CommandRouter {
                event_tx: event_tx.clone(),
                effect_handler: Rc::clone(&effect_handler),
                runtime: runtime.clone(),
                resources: Rc::clone(&resources),
                activity: activity.clone(),
                tasks: TaskRegistry::new(
                    Rc::clone(&active_tasks),
                    Rc::clone(&untracked_tasks),
                    Rc::clone(&last_termination),
                ),
                deferred_events: Rc::clone(&deferred_events),
                pending_errors: Rc::clone(&pending_errors),
                deferred_event_overflow_policy: Rc::clone(&deferred_event_overflow_policy),
                deferred_event_overflow_count: Rc::clone(&deferred_event_overflow_count),
                closed: Rc::clone(&closed),
                progress_epoch: Rc::clone(&progress_epoch),
                last_termination: Rc::clone(&last_termination),
            };
            let handle = runtime.spawn(async move {
                let _lifecycle_guard = lifecycle_guard;
                let command = future.await;
                if closed.get() {
                    return;
                }

                if !abortable_task_is_alive(&active_tasks, owner_for_task.as_ref(), lease_id, token)
                {
                    return;
                }

                let Some(command) = command else {
                    return;
                };

                router_for_task.route_spawned_command(command);
            });

            Ok(Some(SpawnedTask {
                lease_id,
                owner,
                token,
                handle: TaskHandle::wait_for_completion(
                    handle,
                    Some(TaskCancelHandle::Blocking(cancel)),
                ),
                process_controller: None,
                is_process_task: false,
            }))
        }
    }
}

fn cancel_blocking_abortable_task<E, X>(task: &Task<E, X>) -> Result<(), ShellError> {
    match task {
        Task::Blocking(_) => Err(ShellError::AbortableBlockingTask),
        _ => Ok(()),
    }
}

impl<E, X, Resources> CommandRouter<E, X, Resources>
where
    E: 'static,
    X: 'static,
    Resources: 'static,
{
    fn route_spawned_command(&self, command: Command<E, X>) {
        if self.closed.get() {
            return;
        }

        mark_progress(&self.progress_epoch);

        let mut queue = VecDeque::new();
        queue.push_back(command);

        match self.route_command_iterative(&mut queue) {
            Ok(()) => {}
            Err(err) => {
                self.pending_errors.borrow_mut().push_back(err);
            }
        }
    }
}

fn flush_deferred_events<E>(
    event_tx: &EventSender<E>,
    deferred_events: &DeferredEvents<E>,
) -> usize {
    let mut flushed = 0usize;

    loop {
        let Some(event) = deferred_events.borrow_mut().pop_front() else {
            break;
        };

        match event_tx.try_send_owned(event) {
            Ok(()) => {
                flushed = flushed.saturating_add(1);
            }
            Err(crossbeam_channel::TrySendError::Full(event)) => {
                deferred_events.borrow_mut().push_front(event);
                break;
            }
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                report_spawned_event_drop(
                    "event channel disconnected while flushing deferred events",
                );
                break;
            }
        }
    }

    flushed
}

fn report_spawned_event_drop(message: &str) {
    #[cfg(feature = "tracing")]
    tracing::warn!("{message}");

    #[cfg(not(feature = "tracing"))]
    {
        let _ = message;
    }
}
