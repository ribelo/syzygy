use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use futures::stream::StreamExt;

use crate::activity::Activity;
use crate::command::{CancelId, Command, CommandStep};
use crate::core::EventSender;
use crate::dependency::ResourceMap;
use crate::error::ShellError;
use crate::executor::Task;
use crate::extract::EffectContext;

pub(crate) type EffectHandlerFn<E, X> = Rc<dyn for<'a> Fn(X, &EffectContext<'a>) -> Task<E, X>>;
type TaskHandle = compio::runtime::JoinHandle<()>;

#[derive(Debug)]
struct ActiveTaskEntry {
    token: u64,
    handle: TaskHandle,
}

type ActiveTasks = Rc<RefCell<HashMap<CancelId, ActiveTaskEntry>>>;
type UntrackedTasks = Rc<RefCell<Vec<TaskHandle>>>;
type DeferredEvents<E> = Rc<RefCell<VecDeque<E>>>;
type ClosedFlag = Rc<Cell<bool>>;

#[derive(Debug)]
struct SpawnedTask {
    token: Option<u64>,
    handle: TaskHandle,
}

#[derive(Debug)]
struct TaskLifecycleGuard {
    activity: Activity,
    active_tasks: ActiveTasks,
    tracked_slot: Option<CancelId>,
    tracked_token: Option<u64>,
}

impl TaskLifecycleGuard {
    fn new(
        activity: Activity,
        active_tasks: ActiveTasks,
        tracked_slot: Option<CancelId>,
        tracked_token: Option<u64>,
    ) -> Self {
        Self {
            activity,
            active_tasks,
            tracked_slot,
            tracked_token,
        }
    }
}

impl Drop for TaskLifecycleGuard {
    fn drop(&mut self) {
        self.activity.dec();
        if let (Some(id), Some(token)) = (self.tracked_slot, self.tracked_token) {
            cleanup_tracked_slot_if_current(&self.active_tasks, id, token);
        }
    }
}

static NEXT_TASK_TOKEN: AtomicU64 = AtomicU64::new(1);

fn next_task_token() -> u64 {
    NEXT_TASK_TOKEN.fetch_add(1, Ordering::Relaxed)
}

const MAX_DEFERRED_EVENTS: usize = 65_536;
const MAX_INLINE_STREAM_COMMANDS: usize = 10_000;
const DEFERRED_STREAM_BACKPRESSURE_THRESHOLD: usize = 256;
const DEFERRED_STREAM_BACKPRESSURE_SLEEP: Duration = Duration::from_millis(1);
const EXECUTOR_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const EXECUTOR_POLL_SLICE: Duration = Duration::from_millis(5);

pub struct Shell<E, X>
where
    E: 'static,
    X: 'static,
{
    event_tx: EventSender<E>,
    effect_handler: EffectHandlerFn<E, X>,
    resources: Rc<ResourceMap>,
    activity: Activity,
    active_tasks: ActiveTasks,
    untracked_tasks: UntrackedTasks,
    deferred_events: DeferredEvents<E>,
    closed: ClosedFlag,
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
    ) -> Self {
        Self {
            event_tx,
            effect_handler,
            resources: Rc::new(resources),
            activity: Activity::new(),
            active_tasks: Rc::new(RefCell::new(HashMap::new())),
            untracked_tasks: Rc::new(RefCell::new(Vec::new())),
            deferred_events: Rc::new(RefCell::new(VecDeque::new())),
            closed: Rc::new(Cell::new(false)),
        }
    }

    pub fn dispatch_command(&mut self, command: Command<E, X>) -> Result<(), ShellError> {
        if self.closed.get() {
            return Ok(());
        }

        route_command_iterative(
            command,
            &self.event_tx,
            &self.effect_handler,
            &self.resources,
            &self.activity,
            &self.active_tasks,
            &self.untracked_tasks,
            &self.deferred_events,
            &self.closed,
        )
    }

    pub fn drain(&mut self) -> Result<usize, ShellError> {
        prune_finished_untracked_tasks(&self.untracked_tasks);

        let mut progressed = flush_deferred_events(&self.event_tx, &self.deferred_events);
        let _ = compio::runtime::Runtime::try_with_current(|runtime| {
            let mut ran_work = runtime.run();

            if self.activity.load() > 0 {
                runtime.poll_with(Some(Duration::ZERO));
                ran_work |= runtime.run();
            }

            if ran_work {
                progressed = progressed.saturating_add(1);
            }
        });

        prune_finished_untracked_tasks(&self.untracked_tasks);

        Ok(progressed)
    }

    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.activity.load() == 0
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.get()
    }

    pub fn shutdown(&mut self) {
        self.closed.set(true);

        let tracked_tasks = {
            let mut active_tasks = self.active_tasks.borrow_mut();
            std::mem::take(&mut *active_tasks)
        };
        let untracked_tasks = {
            let mut untracked_tasks = self.untracked_tasks.borrow_mut();
            std::mem::take(&mut *untracked_tasks)
        };
        self.deferred_events.borrow_mut().clear();

        drop(tracked_tasks);
        drop(untracked_tasks);
    }

    pub fn wait_for_executors(&self) {
        prune_finished_untracked_tasks(&self.untracked_tasks);

        if self.activity.load() == 0 {
            return;
        }

        let deadline = Instant::now()
            .checked_add(EXECUTOR_SHUTDOWN_TIMEOUT)
            .unwrap_or_else(Instant::now);

        while self.activity.load() > 0 {
            prune_finished_untracked_tasks(&self.untracked_tasks);

            let now = Instant::now();
            if now >= deadline {
                report_spawned_event_drop("timed out while waiting for executors to drain");
                break;
            }

            let remaining = deadline.saturating_duration_since(now);
            let wait_slice = remaining.min(EXECUTOR_POLL_SLICE);

            if compio::runtime::Runtime::try_with_current(|runtime| {
                let mut ran_work = runtime.run();
                if self.activity.load() > 0 {
                    runtime.poll_with(Some(wait_slice));
                    ran_work |= runtime.run();
                }
                ran_work
            })
            .is_ok()
            {
                continue;
            }

            if self.activity.wait_until_zero(wait_slice) {
                break;
            }
        }

        prune_finished_untracked_tasks(&self.untracked_tasks);
    }
}

fn cancel_tracked_slot(active_tasks: &ActiveTasks, id: CancelId) {
    let removed_entry = {
        let mut active_tasks = active_tasks.borrow_mut();
        active_tasks.remove(&id)
    };

    if let Some(entry) = removed_entry {
        drop(entry.handle);
    }
}

fn cleanup_tracked_slot_if_current(active_tasks: &ActiveTasks, id: CancelId, token: u64) {
    if let std::collections::hash_map::Entry::Occupied(entry) = active_tasks.borrow_mut().entry(id)
    {
        if entry.get().token == token {
            entry.remove();
        }
    }
}

fn tracked_slot_is_current(
    active_tasks: &ActiveTasks,
    tracked_slot: Option<CancelId>,
    tracked_token: Option<u64>,
) -> bool {
    match (tracked_slot, tracked_token) {
        (Some(id), Some(token)) => active_tasks
            .borrow()
            .get(&id)
            .is_some_and(|entry| entry.token == token),
        (None, None) => true,
        _ => false,
    }
}

fn prune_finished_untracked_tasks(untracked_tasks: &UntrackedTasks) {
    untracked_tasks
        .borrow_mut()
        .retain(|handle| !handle.is_finished());
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
    initial_command: Command<E, X>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    closed: &ClosedFlag,
) -> Result<(), ShellError>
where
    E: 'static,
    X: 'static,
{
    let mut queue = VecDeque::from([initial_command]);

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
                        resources,
                        activity,
                        active_tasks,
                        untracked_tasks,
                        deferred_events,
                        closed,
                        &mut queue,
                    ) {
                        untracked_tasks.borrow_mut().push(spawned.handle);
                    }
                }
                CommandStep::Tracked { id, effect } => {
                    cancel_tracked_slot(active_tasks, id);
                    if let Some(spawned) = run_effect_task(
                        effect,
                        Some(id),
                        event_tx,
                        effect_handler,
                        resources,
                        activity,
                        active_tasks,
                        untracked_tasks,
                        deferred_events,
                        closed,
                        &mut queue,
                    ) {
                        if let Some(token) = spawned.token {
                            active_tasks.borrow_mut().insert(
                                id,
                                ActiveTaskEntry {
                                    token,
                                    handle: spawned.handle,
                                },
                            );
                        } else {
                            untracked_tasks.borrow_mut().push(spawned.handle);
                        }
                    }
                }
                CommandStep::Cancel { id } => {
                    cancel_tracked_slot(active_tasks, id);
                }
            }
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_effect_task<E, X>(
    effect: X,
    tracked_slot: Option<CancelId>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    closed: &ClosedFlag,
    queue: &mut VecDeque<Command<E, X>>,
) -> Option<SpawnedTask>
where
    E: 'static,
    X: 'static,
{
    let ctx = EffectContext::new(resources.as_ref());
    let task = effect_handler(effect, &ctx);
    spawn_task(
        task,
        tracked_slot,
        event_tx,
        effect_handler,
        resources,
        activity,
        active_tasks,
        untracked_tasks,
        deferred_events,
        closed,
        queue,
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_task<E, X>(
    task: Task<E, X>,
    tracked_slot: Option<CancelId>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    closed: &ClosedFlag,
    queue: &mut VecDeque<Command<E, X>>,
) -> Option<SpawnedTask>
where
    E: 'static,
    X: 'static,
{
    match task {
        Task::None => None,
        Task::Resolved(command) => {
            queue.push_back(command);
            None
        }
        Task::Future(future) => {
            let has_runtime = compio::runtime::Runtime::try_with_current(|_| ()).is_ok();
            if !has_runtime {
                let command = futures::executor::block_on(future);
                queue.push_back(command);
                return None;
            }

            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = Rc::clone(resources);
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let untracked_tasks = Rc::clone(untracked_tasks);
            let deferred_events = Rc::clone(deferred_events);
            let closed = Rc::clone(closed);
            let token = tracked_slot.map(|_| next_task_token());
            activity.inc();
            let lifecycle_guard = TaskLifecycleGuard::new(
                activity.clone(),
                Rc::clone(&active_tasks),
                tracked_slot,
                token,
            );

            let handle = compio::runtime::spawn(async move {
                let _lifecycle_guard = lifecycle_guard;
                let command = future.await;
                if closed.get() {
                    return;
                }

                if !tracked_slot_is_current(&active_tasks, tracked_slot, token) {
                    return;
                }

                route_spawned_command(
                    command,
                    &event_tx,
                    &effect_handler,
                    &resources,
                    &activity,
                    &active_tasks,
                    &untracked_tasks,
                    &deferred_events,
                    &closed,
                );
            });

            Some(SpawnedTask { token, handle })
        }
        Task::Stream(stream) => {
            let has_runtime = compio::runtime::Runtime::try_with_current(|_| ()).is_ok();
            if !has_runtime {
                let commands = futures::executor::block_on(
                    stream
                        .take(MAX_INLINE_STREAM_COMMANDS + 1)
                        .collect::<Vec<_>>(),
                );
                if commands.len() > MAX_INLINE_STREAM_COMMANDS {
                    report_spawned_event_drop(
                        "inline stream command limit reached; dropping remaining commands",
                    );
                }

                for command in commands.into_iter().take(MAX_INLINE_STREAM_COMMANDS) {
                    queue.push_back(command);
                }
                return None;
            }

            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = Rc::clone(resources);
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let untracked_tasks = Rc::clone(untracked_tasks);
            let deferred_events = Rc::clone(deferred_events);
            let closed = Rc::clone(closed);
            let token = tracked_slot.map(|_| next_task_token());
            activity.inc();
            let lifecycle_guard = TaskLifecycleGuard::new(
                activity.clone(),
                Rc::clone(&active_tasks),
                tracked_slot,
                token,
            );

            let handle = compio::runtime::spawn(async move {
                let _lifecycle_guard = lifecycle_guard;
                futures::pin_mut!(stream);
                while let Some(command) = stream.next().await {
                    if closed.get() {
                        break;
                    }

                    if !tracked_slot_is_current(&active_tasks, tracked_slot, token) {
                        break;
                    }

                    route_spawned_command(
                        command,
                        &event_tx,
                        &effect_handler,
                        &resources,
                        &activity,
                        &active_tasks,
                        &untracked_tasks,
                        &deferred_events,
                        &closed,
                    );

                    if closed.get() {
                        break;
                    }

                    if !tracked_slot_is_current(&active_tasks, tracked_slot, token) {
                        break;
                    }

                    let backlog = event_tx
                        .len()
                        .saturating_add(deferred_events.borrow().len());
                    if backlog >= DEFERRED_STREAM_BACKPRESSURE_THRESHOLD {
                        compio::runtime::time::sleep(DEFERRED_STREAM_BACKPRESSURE_SLEEP).await;
                    }
                }
            });

            Some(SpawnedTask { token, handle })
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn route_spawned_command<E, X>(
    command: Command<E, X>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &Rc<ResourceMap>,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    untracked_tasks: &UntrackedTasks,
    deferred_events: &DeferredEvents<E>,
    closed: &ClosedFlag,
) where
    E: 'static,
    X: 'static,
{
    if closed.get() {
        return;
    }

    if route_command_iterative(
        command,
        event_tx,
        effect_handler,
        resources,
        activity,
        active_tasks,
        untracked_tasks,
        deferred_events,
        closed,
    )
    .is_err()
    {
        report_spawned_event_drop("event channel disconnected while routing spawned command");
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
