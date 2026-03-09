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
use crate::resource::ResourceMap;
use crate::subscription::{
    ErasedSubscriptionMapper, Subscription, SubscriptionDrivers, SubscriptionEntry,
    SubscriptionKey, SubscriptionSpec,
};
use crate::syzygy::UnhandledEffectPolicy;

pub(crate) type EffectHandlerFn<E, X> =
    Rc<dyn for<'a> Fn(X, &EffectContext<'a>) -> Result<Task<E, X>, ShellError>>;
pub(crate) type SubscriptionHandlerFn<E, X, M> =
    Rc<dyn Fn(&crate::extract::SubscriptionContext<M>) -> Subscription<E, X>>;

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
}

type ActiveTasks = Rc<RefCell<HashMap<u64, ActiveTaskEntry>>>;
type UntrackedTasks = Rc<RefCell<Vec<TaskHandle>>>;
type DeferredEvents<E> = Rc<RefCell<VecDeque<E>>>;
type PendingErrors = Rc<RefCell<VecDeque<ShellError>>>;
type ClosedFlag = Rc<Cell<bool>>;
type ProgressEpoch = Rc<Cell<u64>>;
type ActiveSubscriptions<E, X> =
    Rc<RefCell<HashMap<SubscriptionKey, ActiveSubscriptionEntry<E, X>>>>;

struct SpawnedTask {
    lease_id: Option<u64>,
    owner: Option<TaskLeaseWeak>,
    token: Option<u64>,
    handle: TaskHandle,
    process_controller: Option<ProcessController>,
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
    lease_id: Option<u64>,
    token: Option<u64>,
}

impl TaskLifecycleGuard {
    fn new(
        activity: Activity,
        active_tasks: ActiveTasks,
        lease_id: Option<u64>,
        token: Option<u64>,
    ) -> Self {
        Self {
            activity,
            active_tasks,
            lease_id,
            token,
        }
    }
}

impl Drop for TaskLifecycleGuard {
    fn drop(&mut self) {
        self.activity.dec();
        if let (Some(lease_id), Some(token)) = (self.lease_id, self.token) {
            cleanup_active_task_if_current(&self.active_tasks, lease_id, token);
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
        key: SubscriptionKey,
        token: u64,
    ) -> Self {
        Self {
            activity,
            active_subscriptions,
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
        cleanup_active_subscription_if_current(&self.active_subscriptions, &self.key, self.token);
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

pub struct Shell<E, X, M = ()>
where
    E: 'static,
    X: 'static,
{
    event_tx: EventSender<E>,
    effect_handler: EffectHandlerFn<E, X>,
    subscription_handler: Option<SubscriptionHandlerFn<E, X, M>>,
    subscription_drivers: Rc<SubscriptionDrivers>,
    runtime: crate::runtime::Runtime,
    resources: Rc<ResourceMap>,
    activity: Activity,
    active_tasks: ActiveTasks,
    active_subscriptions: ActiveSubscriptions<E, X>,
    untracked_tasks: UntrackedTasks,
    deferred_events: DeferredEvents<E>,
    pending_errors: PendingErrors,
    closed: ClosedFlag,
    progress_epoch: ProgressEpoch,
    unhandled_effects_policy: Rc<Cell<UnhandledEffectPolicy>>,
    queue: VecDeque<Command<E, X>>,
}

impl<E, X> Shell<E, X, ()>
where
    E: 'static,
    X: 'static,
{
    pub fn new(
        event_tx: EventSender<E>,
        effect_handler: EffectHandlerFn<E, X>,
        resources: ResourceMap,
        runtime: crate::runtime::Runtime,
    ) -> Self {
        Self::with_subscriptions(
            event_tx,
            effect_handler,
            None,
            SubscriptionDrivers::new(),
            resources,
            runtime,
            Rc::new(Cell::new(UnhandledEffectPolicy::Error)),
        )
    }
}

impl<E, X, M> Shell<E, X, M>
where
    E: 'static,
    X: 'static,
    M: 'static,
{
    pub(crate) fn with_subscriptions(
        event_tx: EventSender<E>,
        effect_handler: EffectHandlerFn<E, X>,
        subscription_handler: Option<SubscriptionHandlerFn<E, X, M>>,
        subscription_drivers: SubscriptionDrivers,
        resources: ResourceMap,
        runtime: crate::runtime::Runtime,
        unhandled_effects_policy: Rc<Cell<UnhandledEffectPolicy>>,
    ) -> Self {
        Self {
            event_tx,
            effect_handler,
            subscription_handler,
            subscription_drivers: Rc::new(subscription_drivers),
            runtime,
            resources: Rc::new(resources),
            activity: Activity::new(),
            active_tasks: Rc::new(RefCell::new(HashMap::new())),
            active_subscriptions: Rc::new(RefCell::new(HashMap::new())),
            untracked_tasks: Rc::new(RefCell::new(Vec::new())),
            deferred_events: Rc::new(RefCell::new(VecDeque::new())),
            pending_errors: Rc::new(RefCell::new(VecDeque::new())),
            closed: Rc::new(Cell::new(false)),
            progress_epoch: Rc::new(Cell::new(0)),
            unhandled_effects_policy,
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

        prune_orphaned_active_tasks(&self.active_tasks, &self.untracked_tasks);
        self.queue.clear();
        self.queue.push_back(command);

        route_command_iterative(
            &mut self.queue,
            &self.event_tx,
            &self.effect_handler,
            &self.runtime,
            &self.resources,
            &self.activity,
            &self.active_tasks,
            &self.untracked_tasks,
            &self.deferred_events,
            &self.pending_errors,
            &self.closed,
            &self.progress_epoch,
        )?;

        if let Some(err) = take_pending_error(&self.pending_errors) {
            return Err(err);
        }

        Ok(())
    }

    pub fn drain(&mut self) -> Result<usize, ShellError> {
        if let Some(err) = take_pending_error(&self.pending_errors) {
            return Err(err);
        }

        prune_orphaned_active_tasks(&self.active_tasks, &self.untracked_tasks);
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

        prune_orphaned_active_tasks(&self.active_tasks, &self.untracked_tasks);
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

        Ok(usize::from(made_progress))
    }

    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.activity.load() == 0
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
    pub fn unhandled_effects_policy(&self) -> UnhandledEffectPolicy {
        self.unhandled_effects_policy.get()
    }

    pub fn set_unhandled_effects_policy(&mut self, policy: UnhandledEffectPolicy) {
        self.unhandled_effects_policy.set(policy);
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
                cancel_active_subscription(&self.active_subscriptions, &self.untracked_tasks, &key);
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

            cancel_active_subscription(&self.active_subscriptions, &self.untracked_tasks, &key);
            self.start_subscription(entry)?;
            changes = changes.saturating_add(1);
        }

        for entry in desired_by_key.into_values() {
            self.start_subscription(entry)?;
            changes = changes.saturating_add(1);
        }

        if let Some(err) = take_pending_error(&self.pending_errors) {
            return Err(err);
        }

        Ok(changes)
    }

    pub fn shutdown(&mut self) {
        self.closed.set(true);
        self.deferred_events.borrow_mut().clear();
        drain_active_tasks_for_shutdown(&self.active_tasks, &self.untracked_tasks);
        drain_active_subscriptions_for_shutdown(&self.active_subscriptions, &self.untracked_tasks);
        request_shutdown_for_untracked_tasks(&self.untracked_tasks);
    }

    pub fn wait_for_executors(&self) {
        prune_orphaned_active_tasks(&self.active_tasks, &self.untracked_tasks);
        prune_finished_untracked_tasks(&self.untracked_tasks);

        if self.activity.load() == 0 {
            return;
        }

        while self.activity.load() > 0 {
            prune_orphaned_active_tasks(&self.active_tasks, &self.untracked_tasks);
            prune_finished_untracked_tasks(&self.untracked_tasks);
            self.runtime.park(EXECUTOR_POLL_SLICE);
        }

        prune_orphaned_active_tasks(&self.active_tasks, &self.untracked_tasks);
        prune_finished_untracked_tasks(&self.untracked_tasks);
    }

    pub(crate) fn park_runtime(&self, duration: Duration) {
        self.runtime.park(duration);
    }

    fn start_subscription(&mut self, entry: SubscriptionEntry<E, X>) -> Result<(), ShellError> {
        let (key, driver_id, driver_name, spec, mapper) = entry.into_parts();
        if !self.subscription_drivers.contains(driver_id) {
            return Err(ShellError::MissingSubscriptionDriver(driver_name.into()));
        }

        let mapper = Rc::new(RefCell::new(mapper));
        let token = next_task_token();
        let handle = spawn_subscription_runtime_task(
            key.clone(),
            Rc::clone(&mapper),
            token,
            Rc::clone(&self.subscription_drivers),
            driver_id,
            driver_name,
            spec.clone(),
            &self.event_tx,
            &self.effect_handler,
            &self.runtime,
            &self.resources,
            &self.activity,
            &self.active_tasks,
            &self.active_subscriptions,
            &self.untracked_tasks,
            &self.deferred_events,
            &self.pending_errors,
            &self.closed,
            &self.progress_epoch,
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
}

fn keep_task_after_cancel(untracked_tasks: &UntrackedTasks, handle: TaskHandle) {
    if let Some(handle) = handle.request_cancel() {
        untracked_tasks.borrow_mut().push(handle);
    }
}

fn cleanup_active_subscription_if_current<E, X>(
    active_subscriptions: &ActiveSubscriptions<E, X>,
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

fn cancel_active_task(active_tasks: &ActiveTasks, untracked_tasks: &UntrackedTasks, lease_id: u64) {
    let removed_entry = {
        let mut active_tasks = active_tasks.borrow_mut();
        active_tasks.remove(&lease_id)
    };

    if let Some(entry) = removed_entry {
        keep_task_after_cancel(untracked_tasks, entry.handle);
    }
}

fn cancel_active_subscription<E, X>(
    active_subscriptions: &ActiveSubscriptions<E, X>,
    untracked_tasks: &UntrackedTasks,
    key: &SubscriptionKey,
) where
    E: 'static,
    X: 'static,
{
    let removed_entry = {
        let mut active_subscriptions = active_subscriptions.borrow_mut();
        active_subscriptions.remove(key)
    };

    if let Some(entry) = removed_entry {
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
    lease: TaskLease,
    bytes: Vec<u8>,
) -> Result<(), ShellError> {
    let lease_id = lease.id();
    with_process_controller(active_tasks, lease_id, |controller| {
        controller.write(lease_id, bytes)
    })
}

fn close_active_process_stdin(
    active_tasks: &ActiveTasks,
    lease: TaskLease,
) -> Result<(), ShellError> {
    let lease_id = lease.id();
    with_process_controller(active_tasks, lease_id, |controller| {
        controller.close_stdin(lease_id)
    })
}

fn cleanup_active_task_if_current(active_tasks: &ActiveTasks, lease_id: u64, token: u64) {
    if let std::collections::hash_map::Entry::Occupied(entry) =
        active_tasks.borrow_mut().entry(lease_id)
    {
        if entry.get().token == token {
            entry.remove();
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

fn prune_orphaned_active_tasks(active_tasks: &ActiveTasks, untracked_tasks: &UntrackedTasks) {
    let orphaned = {
        let active_tasks = active_tasks.borrow();
        active_tasks
            .values()
            .filter(|entry| !entry.owner.has_owner())
            .map(|entry| entry.owner.id())
            .collect::<Vec<_>>()
    };

    for lease_id in orphaned {
        cancel_active_task(active_tasks, untracked_tasks, lease_id);
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

fn drain_active_tasks_for_shutdown(active_tasks: &ActiveTasks, untracked_tasks: &UntrackedTasks) {
    let drained = {
        let mut active_tasks = active_tasks.borrow_mut();
        std::mem::take(&mut *active_tasks)
    };

    for entry in drained.into_values() {
        keep_task_after_cancel(untracked_tasks, entry.handle);
    }
}

fn drain_active_subscriptions_for_shutdown<E, X>(
    active_subscriptions: &ActiveSubscriptions<E, X>,
    untracked_tasks: &UntrackedTasks,
) where
    E: 'static,
    X: 'static,
{
    let drained = {
        let mut active_subscriptions = active_subscriptions.borrow_mut();
        std::mem::take(&mut *active_subscriptions)
    };

    for entry in drained.into_values() {
        keep_task_after_cancel(untracked_tasks, entry.handle);
    }
}

fn request_shutdown_for_untracked_tasks(untracked_tasks: &UntrackedTasks) {
    let drained = {
        let mut untracked_tasks = untracked_tasks.borrow_mut();
        std::mem::take(&mut *untracked_tasks)
    };

    let kept = drained
        .into_iter()
        .filter_map(TaskHandle::request_cancel)
        .collect::<Vec<_>>();
    untracked_tasks.borrow_mut().extend(kept);
}

fn push_deferred_event<E>(deferred_events: &DeferredEvents<E>, event: E) {
    let mut deferred_events = deferred_events.borrow_mut();
    if deferred_events.len() >= MAX_DEFERRED_EVENTS {
        report_spawned_event_drop("dropping deferred event because backlog reached hard limit");
        return;
    }

    deferred_events.push_back(event);
}

#[allow(clippy::too_many_arguments)]
fn route_command_iterative<E, X>(
    queue: &mut VecDeque<Command<E, X>>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    runtime: &crate::runtime::Runtime,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    pending_errors: &PendingErrors,
    closed: &ClosedFlag,
    progress_epoch: &ProgressEpoch,
) -> Result<(), ShellError>
where
    E: 'static,
    X: 'static,
{
    while let Some(command) = queue.pop_front() {
        if closed.get() {
            return Ok(());
        }

        for step in command {
            match step {
                CommandStep::Event(event) => {
                    route_event(event_tx, deferred_events, event)
                        .map_err(|_| ShellError::EventChannelClosed)?;
                }
                CommandStep::Effect(effect) => {
                    if let Some(spawned) = run_effect_task(
                        effect,
                        None,
                        event_tx,
                        effect_handler,
                        runtime,
                        resources,
                        activity,
                        active_tasks,
                        untracked_tasks,
                        deferred_events,
                        pending_errors,
                        closed,
                        progress_epoch,
                        queue,
                    )? {
                        untracked_tasks.borrow_mut().push(spawned.handle);
                    }
                }
                CommandStep::Abortable { lease, effect } => {
                    let ctx = EffectContext::new(resources.as_ref());
                    let task = effect_handler(effect, &ctx)?;
                    cancel_blocking_abortable_task(&task)?;
                    cancel_active_task(active_tasks, untracked_tasks, lease.id());
                    if let Some(spawned) = spawn_task(
                        task,
                        Some(lease),
                        event_tx,
                        effect_handler,
                        runtime,
                        resources,
                        activity,
                        active_tasks,
                        untracked_tasks,
                        deferred_events,
                        pending_errors,
                        closed,
                        progress_epoch,
                        queue,
                    )? {
                        if let (Some(lease_id), Some(owner), Some(token)) =
                            (spawned.lease_id, spawned.owner, spawned.token)
                        {
                            active_tasks.borrow_mut().insert(
                                lease_id,
                                ActiveTaskEntry {
                                    owner,
                                    token,
                                    handle: spawned.handle,
                                    process_controller: spawned.process_controller,
                                },
                            );
                        } else {
                            untracked_tasks.borrow_mut().push(spawned.handle);
                        }
                    }
                }
                CommandStep::Cancel { lease } => {
                    cancel_active_task(active_tasks, untracked_tasks, lease.id());
                }
                CommandStep::ProcessWrite { lease, bytes } => {
                    write_to_active_process(active_tasks, lease, bytes)?;
                }
                CommandStep::ProcessCloseStdin { lease } => {
                    close_active_process_stdin(active_tasks, lease)?;
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_effect_task<E, X>(
    effect: X,
    lease: Option<TaskLease>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    runtime: &crate::runtime::Runtime,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    pending_errors: &PendingErrors,
    closed: &ClosedFlag,
    progress_epoch: &ProgressEpoch,
    queue: &mut VecDeque<Command<E, X>>,
) -> Result<Option<SpawnedTask>, ShellError>
where
    E: 'static,
    X: 'static,
{
    let ctx = EffectContext::new(resources.as_ref());
    let task = effect_handler(effect, &ctx)?;
    spawn_task(
        task,
        lease,
        event_tx,
        effect_handler,
        runtime,
        resources,
        activity,
        active_tasks,
        untracked_tasks,
        deferred_events,
        pending_errors,
        closed,
        progress_epoch,
        queue,
    )
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

#[allow(clippy::too_many_arguments)]
fn route_subscription_update<E, X>(
    mapper: &Rc<RefCell<Box<dyn ErasedSubscriptionMapper<E, X>>>>,
    update: Box<dyn std::any::Any>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    runtime: &crate::runtime::Runtime,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    pending_errors: &PendingErrors,
    closed: &ClosedFlag,
    progress_epoch: &ProgressEpoch,
) where
    E: 'static,
    X: 'static,
{
    let Some(command) = mapper.borrow_mut().map(update) else {
        return;
    };

    route_spawned_command(
        command,
        event_tx,
        effect_handler,
        runtime,
        resources,
        activity,
        active_tasks,
        untracked_tasks,
        deferred_events,
        pending_errors,
        closed,
        progress_epoch,
    );
}

#[allow(clippy::too_many_arguments)]
fn spawn_subscription_runtime_task<E, X>(
    key: SubscriptionKey,
    mapper: Rc<RefCell<Box<dyn ErasedSubscriptionMapper<E, X>>>>,
    token: u64,
    subscription_drivers: Rc<SubscriptionDrivers>,
    driver_id: TypeId,
    driver_name: &'static str,
    spec: SubscriptionSpec,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    runtime: &crate::runtime::Runtime,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    active_subscriptions: &ActiveSubscriptions<E, X>,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    pending_errors: &PendingErrors,
    closed: &ClosedFlag,
    progress_epoch: &ProgressEpoch,
) -> TaskHandle
where
    E: 'static,
    X: 'static,
{
    let runtime = runtime.clone();
    let event_tx = event_tx.clone();
    let effect_handler = Rc::clone(effect_handler);
    let resources = Rc::clone(resources);
    let activity = activity.clone();
    let active_tasks = Rc::clone(active_tasks);
    let active_subscriptions = Rc::clone(active_subscriptions);
    let untracked_tasks = Rc::clone(untracked_tasks);
    let deferred_events = Rc::clone(deferred_events);
    let pending_errors = Rc::clone(pending_errors);
    let closed = Rc::clone(closed);
    let progress_epoch = Rc::clone(progress_epoch);
    let mapper_for_task = Rc::clone(&mapper);
    activity.inc();
    let lifecycle_guard = SubscriptionLifecycleGuard::new(
        activity.clone(),
        Rc::clone(&active_subscriptions),
        key.clone(),
        token,
    );
    let (cancel_tx, cancel_rx) = oneshot::channel();
    let mut cancel_future: LocalBoxFuture<'static, ()> = Box::pin(async move {
        let _ = cancel_rx.await;
    });

    let runtime_for_task = runtime.clone();
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

            route_subscription_update(
                &mapper_for_task,
                update,
                &event_tx,
                &effect_handler,
                &runtime_for_task,
                &resources,
                &activity,
                &active_tasks,
                &untracked_tasks,
                &deferred_events,
                &pending_errors,
                &closed,
                &progress_epoch,
            );
        }
    });

    TaskHandle::wait_for_completion(
        handle,
        Some(TaskCancelHandle::Subscription(Some(cancel_tx))),
    )
}

#[allow(clippy::too_many_arguments)]
fn route_process_update<E, X>(
    update: ProcessUpdate,
    on_update: &mut dyn FnMut(ProcessUpdate) -> Option<Command<E, X>>,
    active_tasks: &ActiveTasks,
    lease_id: Option<u64>,
    token: Option<u64>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    runtime: &crate::runtime::Runtime,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    pending_errors: &PendingErrors,
    closed: &ClosedFlag,
    progress_epoch: &ProgressEpoch,
) where
    E: 'static,
    X: 'static,
{
    if matches!(update, ProcessUpdate::Exited(_)) {
        if let (Some(lease_id), Some(token)) = (lease_id, token) {
            cleanup_active_task_if_current(active_tasks, lease_id, token);
        }
    }

    if let Some(command) = on_update(update) {
        route_spawned_command(
            command,
            event_tx,
            effect_handler,
            runtime,
            resources,
            activity,
            active_tasks,
            untracked_tasks,
            deferred_events,
            pending_errors,
            closed,
            progress_epoch,
        );
    }
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

#[allow(clippy::too_many_arguments)]
fn spawn_process_runtime_task<E, X>(
    spec: ProcessSpec,
    mut on_update: Box<dyn FnMut(ProcessUpdate) -> Option<Command<E, X>> + 'static>,
    mut child: async_process::Child,
    control_rx: Option<ProcessControlReceiver>,
    process_controller: Option<ProcessController>,
    lease_id: Option<u64>,
    owner: Option<TaskLeaseWeak>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    runtime: &crate::runtime::Runtime,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    pending_errors: &PendingErrors,
    closed: &ClosedFlag,
    progress_epoch: &ProgressEpoch,
) -> SpawnedTask
where
    E: 'static,
    X: 'static,
{
    let runtime = runtime.clone();
    let event_tx = event_tx.clone();
    let effect_handler = Rc::clone(effect_handler);
    let resources = Rc::clone(resources);
    let activity = activity.clone();
    let active_tasks = Rc::clone(active_tasks);
    let untracked_tasks = Rc::clone(untracked_tasks);
    let deferred_events = Rc::clone(deferred_events);
    let pending_errors = Rc::clone(pending_errors);
    let closed = Rc::clone(closed);
    let progress_epoch = Rc::clone(progress_epoch);
    let token = lease_id.map(|_| next_task_token());
    let owner_for_task = owner.clone();
    activity.inc();
    let lifecycle_guard =
        TaskLifecycleGuard::new(activity.clone(), Rc::clone(&active_tasks), lease_id, token);

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

    let runtime_for_task = runtime.clone();
    let handle = runtime.spawn(async move {
        let _lifecycle_guard = lifecycle_guard;
        let mut control_rx = control_rx;
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
                    route_process_update(
                        update,
                        &mut on_update,
                        &active_tasks,
                        lease_id,
                        token,
                        &event_tx,
                        &effect_handler,
                        &runtime_for_task,
                        &resources,
                        &activity,
                        &untracked_tasks,
                        &deferred_events,
                        &pending_errors,
                        &closed,
                        &progress_epoch,
                    );
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
                            route_process_update(
                                ProcessUpdate::Stdout(frame),
                                &mut on_update,
                                &active_tasks,
                                lease_id,
                                token,
                                &event_tx,
                                &effect_handler,
                                &runtime_for_task,
                                &resources,
                                &activity,
                                &untracked_tasks,
                                &deferred_events,
                                &pending_errors,
                                &closed,
                                &progress_epoch,
                            );
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
                            route_process_update(
                                ProcessUpdate::Stderr(frame),
                                &mut on_update,
                                &active_tasks,
                                lease_id,
                                token,
                                &event_tx,
                                &effect_handler,
                                &runtime_for_task,
                                &resources,
                                &activity,
                                &untracked_tasks,
                                &deferred_events,
                                &pending_errors,
                                &closed,
                                &progress_epoch,
                            );
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
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_task<E, X>(
    task: Task<E, X>,
    lease: Option<TaskLease>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    runtime: &crate::runtime::Runtime,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    pending_errors: &PendingErrors,
    closed: &ClosedFlag,
    progress_epoch: &ProgressEpoch,
    queue: &mut VecDeque<Command<E, X>>,
) -> Result<Option<SpawnedTask>, ShellError>
where
    E: 'static,
    X: 'static,
{
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

            let runtime = runtime.clone();
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = Rc::clone(resources);
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let untracked_tasks = Rc::clone(untracked_tasks);
            let deferred_events = Rc::clone(deferred_events);
            let pending_errors = Rc::clone(pending_errors);
            let closed = Rc::clone(closed);
            let progress_epoch = Rc::clone(progress_epoch);
            let token = lease_id.map(|_| next_task_token());
            let owner_for_task = owner.clone();
            activity.inc();
            let lifecycle_guard = TaskLifecycleGuard::new(
                activity.clone(),
                Rc::clone(&active_tasks),
                lease_id,
                token,
            );

            let runtime_for_task = runtime.clone();
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

                route_spawned_command(
                    command,
                    &event_tx,
                    &effect_handler,
                    &runtime_for_task,
                    &resources,
                    &activity,
                    &active_tasks,
                    &untracked_tasks,
                    &deferred_events,
                    &pending_errors,
                    &closed,
                    &progress_epoch,
                );
            });

            Ok(Some(SpawnedTask {
                lease_id,
                owner,
                token,
                handle: TaskHandle::abort_on_drop(handle),
                process_controller: None,
            }))
        }
        Task::Stream(stream) => {
            if owner.as_ref().is_some_and(|owner| !owner.has_owner()) {
                return Ok(None);
            }

            let runtime = runtime.clone();
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = Rc::clone(resources);
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let untracked_tasks = Rc::clone(untracked_tasks);
            let deferred_events = Rc::clone(deferred_events);
            let pending_errors = Rc::clone(pending_errors);
            let closed = Rc::clone(closed);
            let progress_epoch = Rc::clone(progress_epoch);
            let token = lease_id.map(|_| next_task_token());
            let owner_for_task = owner.clone();
            activity.inc();
            let lifecycle_guard = TaskLifecycleGuard::new(
                activity.clone(),
                Rc::clone(&active_tasks),
                lease_id,
                token,
            );

            let runtime_for_task = runtime.clone();
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

                    route_spawned_command(
                        command,
                        &event_tx,
                        &effect_handler,
                        &runtime_for_task,
                        &resources,
                        &activity,
                        &active_tasks,
                        &untracked_tasks,
                        &deferred_events,
                        &pending_errors,
                        &closed,
                        &progress_epoch,
                    );

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
            }))
        }
        Task::Process(task) => {
            if owner.as_ref().is_some_and(|owner| !owner.has_owner()) {
                return Ok(None);
            }

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

            Ok(Some(spawn_process_runtime_task(
                spec,
                on_update,
                child,
                control_rx,
                process_controller,
                lease_id,
                owner,
                event_tx,
                effect_handler,
                runtime,
                resources,
                activity,
                active_tasks,
                untracked_tasks,
                deferred_events,
                pending_errors,
                closed,
                progress_epoch,
            )))
        }
        Task::Blocking(task) => {
            if owner.is_some() {
                return Err(ShellError::AbortableBlockingTask);
            }

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
            let closed = Rc::clone(closed);
            let progress_epoch = Rc::clone(progress_epoch);
            activity.inc();
            let lifecycle_guard =
                TaskLifecycleGuard::new(activity.clone(), Rc::clone(&active_tasks), None, None);

            let runtime_for_task = runtime.clone();
            let handle = runtime.spawn(async move {
                let _lifecycle_guard = lifecycle_guard;
                let command = future.await;
                if closed.get() {
                    return;
                }

                route_spawned_command(
                    command,
                    &event_tx,
                    &effect_handler,
                    &runtime_for_task,
                    &resources,
                    &activity,
                    &active_tasks,
                    &untracked_tasks,
                    &deferred_events,
                    &pending_errors,
                    &closed,
                    &progress_epoch,
                );
            });

            Ok(Some(SpawnedTask {
                lease_id: None,
                owner: None,
                token: None,
                handle: TaskHandle::wait_for_completion(handle, None),
                process_controller: None,
            }))
        }
        Task::BlockingCooperative(task) => {
            if owner.as_ref().is_some_and(|owner| !owner.has_owner()) {
                return Ok(None);
            }

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
            let closed = Rc::clone(closed);
            let progress_epoch = Rc::clone(progress_epoch);
            let token = lease_id.map(|_| next_task_token());
            let owner_for_task = owner.clone();
            activity.inc();
            let lifecycle_guard = TaskLifecycleGuard::new(
                activity.clone(),
                Rc::clone(&active_tasks),
                lease_id,
                token,
            );

            let runtime_for_task = runtime.clone();
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

                route_spawned_command(
                    command,
                    &event_tx,
                    &effect_handler,
                    &runtime_for_task,
                    &resources,
                    &activity,
                    &active_tasks,
                    &untracked_tasks,
                    &deferred_events,
                    &pending_errors,
                    &closed,
                    &progress_epoch,
                );
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

#[allow(clippy::too_many_arguments)]
fn route_spawned_command<E, X>(
    command: Command<E, X>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    runtime: &crate::runtime::Runtime,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    pending_errors: &PendingErrors,
    closed: &ClosedFlag,
    progress_epoch: &ProgressEpoch,
) where
    E: 'static,
    X: 'static,
{
    if closed.get() {
        return;
    }

    mark_progress(progress_epoch);

    let mut queue = VecDeque::new();
    queue.push_back(command);

    match route_command_iterative(
        &mut queue,
        event_tx,
        effect_handler,
        runtime,
        resources,
        activity,
        active_tasks,
        untracked_tasks,
        deferred_events,
        pending_errors,
        closed,
        progress_epoch,
    ) {
        Ok(()) => {}
        Err(err) => {
            pending_errors.borrow_mut().push_back(err);
        }
    }
}

fn route_event<E>(
    event_tx: &EventSender<E>,
    deferred_events: &DeferredEvents<E>,
    event: E,
) -> Result<(), crossbeam_channel::TrySendError<E>> {
    if !deferred_events.borrow().is_empty() {
        push_deferred_event(deferred_events, event);
        return Ok(());
    }

    match event_tx.try_send_owned(event) {
        Ok(()) => Ok(()),
        Err(crossbeam_channel::TrySendError::Full(event)) => {
            push_deferred_event(deferred_events, event);
            Ok(())
        }
        Err(crossbeam_channel::TrySendError::Disconnected(event)) => {
            Err(crossbeam_channel::TrySendError::Disconnected(event))
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
