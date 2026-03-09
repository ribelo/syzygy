use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::stream::StreamExt;

use crate::activity::Activity;
use crate::command::{Command, CommandStep, TaskLease, TaskLeaseWeak};
use crate::core::EventSender;
use crate::error::ShellError;
use crate::executor::{BlockingCancelToken, Task};
use crate::extract::EffectContext;
use crate::resource::ResourceMap;

pub(crate) type EffectHandlerFn<E, X> = Rc<dyn for<'a> Fn(X, &EffectContext<'a>) -> Task<E, X>>;

#[derive(Debug)]
enum TaskHandleKind {
    AbortOnDrop,
    WaitForCompletion { cancel: Option<BlockingCancelToken> },
}

#[derive(Debug)]
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
        cancel: Option<BlockingCancelToken>,
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

    fn request_cancel(self) -> Option<Self> {
        match &self.kind {
            TaskHandleKind::AbortOnDrop => None,
            TaskHandleKind::WaitForCompletion { cancel } => {
                if let Some(cancel) = cancel {
                    cancel.cancel();
                }
                Some(self)
            }
        }
    }
}

#[derive(Debug)]
struct ActiveTaskEntry {
    owner: TaskLeaseWeak,
    token: u64,
    handle: TaskHandle,
}

type ActiveTasks = Rc<RefCell<HashMap<u64, ActiveTaskEntry>>>;
type UntrackedTasks = Rc<RefCell<Vec<TaskHandle>>>;
type DeferredEvents<E> = Rc<RefCell<VecDeque<E>>>;
type PendingErrors = Rc<RefCell<VecDeque<ShellError>>>;
type ClosedFlag = Rc<Cell<bool>>;
type ProgressEpoch = Rc<Cell<u64>>;

#[derive(Debug)]
struct SpawnedTask {
    lease_id: Option<u64>,
    owner: Option<TaskLeaseWeak>,
    token: Option<u64>,
    handle: TaskHandle,
}

#[derive(Debug)]
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

pub struct Shell<E, X>
where
    E: 'static,
    X: 'static,
{
    event_tx: EventSender<E>,
    effect_handler: EffectHandlerFn<E, X>,
    runtime: crate::runtime::Runtime,
    resources: Rc<ResourceMap>,
    activity: Activity,
    active_tasks: ActiveTasks,
    untracked_tasks: UntrackedTasks,
    deferred_events: DeferredEvents<E>,
    pending_errors: PendingErrors,
    closed: ClosedFlag,
    progress_epoch: ProgressEpoch,
    queue: VecDeque<Command<E, X>>,
}

impl<E, X> Shell<E, X>
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
        Self {
            event_tx,
            effect_handler,
            runtime,
            resources: Rc::new(resources),
            activity: Activity::new(),
            active_tasks: Rc::new(RefCell::new(HashMap::new())),
            untracked_tasks: Rc::new(RefCell::new(Vec::new())),
            deferred_events: Rc::new(RefCell::new(VecDeque::new())),
            pending_errors: Rc::new(RefCell::new(VecDeque::new())),
            closed: Rc::new(Cell::new(false)),
            progress_epoch: Rc::new(Cell::new(0)),
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

    pub fn shutdown(&mut self) {
        self.closed.set(true);
        self.deferred_events.borrow_mut().clear();
        drain_active_tasks_for_shutdown(&self.active_tasks, &self.untracked_tasks);
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
}

fn keep_task_after_cancel(untracked_tasks: &UntrackedTasks, handle: TaskHandle) {
    if let Some(handle) = handle.request_cancel() {
        untracked_tasks.borrow_mut().push(handle);
    }
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
                    let task = effect_handler(effect, &ctx);
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
    let task = effect_handler(effect, &ctx);
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
            }))
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
                handle: TaskHandle::wait_for_completion(handle, Some(cancel)),
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
