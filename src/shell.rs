use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures::stream::StreamExt;

use crate::activity::Activity;
use crate::command::{CancelId, Command, CommandStep};
use crate::core::EventSender;
use crate::dependency::ResourceMap;
use crate::error::ShellError;
use crate::executor::Task;
use crate::extract::EffectContext;

pub(crate) type EffectHandlerFn<E, X> = Rc<dyn Fn(X, &EffectContext) -> Task<E, X>>;
type TaskHandle = compio::runtime::JoinHandle<()>;

#[derive(Debug)]
struct ActiveTaskEntry {
    token: u64,
    handle: TaskHandle,
}

type ActiveTasks = Rc<RefCell<HashMap<CancelId, ActiveTaskEntry>>>;

#[derive(Debug)]
struct SpawnedTask {
    token: Option<u64>,
    handle: TaskHandle,
}

#[derive(Debug)]
struct ActivityGuard {
    activity: Activity,
}

impl ActivityGuard {
    fn new(activity: Activity) -> Self {
        Self { activity }
    }
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        self.activity.dec();
    }
}

static NEXT_TASK_TOKEN: AtomicU64 = AtomicU64::new(1);

fn next_task_token() -> u64 {
    NEXT_TASK_TOKEN.fetch_add(1, Ordering::Relaxed)
}

pub struct Shell<E, X>
where
    E: 'static,
    X: 'static,
{
    event_tx: EventSender<E>,
    effect_handler: EffectHandlerFn<E, X>,
    resources: ResourceMap,
    activity: Activity,
    active_tasks: ActiveTasks,
    closed: bool,
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
            resources,
            activity: Activity::new(),
            active_tasks: Rc::new(RefCell::new(HashMap::new())),
            closed: false,
        }
    }

    pub fn dispatch_command(&mut self, command: Command<E, X>) -> Result<(), ShellError> {
        if self.closed {
            return Ok(());
        }

        route_command_iterative(
            command,
            &self.event_tx,
            &self.effect_handler,
            &self.resources,
            &self.activity,
            &self.active_tasks,
            true,
        )
    }

    pub fn drain(&mut self) -> Result<usize, ShellError> {
        let mut progressed = 0usize;
        let _ = compio::runtime::Runtime::try_with_current(|runtime| {
            if runtime.run() {
                progressed += 1;
            }

            if self.activity.load() > 0 {
                runtime.poll_with(Some(Duration::ZERO));
                progressed += 1;
            }
        });

        Ok(progressed)
    }

    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.activity.load() == 0
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn shutdown(&mut self) {
        self.closed = true;
        self.active_tasks.borrow_mut().clear();
    }

    pub fn wait_for_executors(&self) {}
}

fn cancel_tracked_slot(active_tasks: &ActiveTasks, id: CancelId) {
    if let Some(entry) = active_tasks.borrow_mut().remove(&id) {
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

fn route_command_iterative<E, X>(
    initial_command: Command<E, X>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &ResourceMap,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    strict_event_send: bool,
) -> Result<(), ShellError>
where
    E: 'static,
    X: 'static,
{
    let mut queue = VecDeque::from([initial_command]);

    while let Some(command) = queue.pop_front() {
        for step in command {
            match step {
                CommandStep::Event(event) => {
                    if strict_event_send {
                        event_tx
                            .send(event)
                            .map_err(|_| ShellError::EventChannelClosed)?;
                    } else {
                        let _ = event_tx.send(event);
                    }
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
                        &mut queue,
                    ) {
                        spawned.handle.detach();
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
                            spawned.handle.detach();
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
    resources: &ResourceMap,
    activity: &Activity,
    active_tasks: &ActiveTasks,
    queue: &mut VecDeque<Command<E, X>>,
) -> Option<SpawnedTask>
where
    E: 'static,
    X: 'static,
{
    let ctx = EffectContext::new(resources.clone());
    let task = effect_handler(effect, &ctx);
    spawn_task(
        task,
        tracked_slot,
        event_tx,
        effect_handler,
        resources,
        activity,
        active_tasks,
        queue,
    )
}

#[allow(clippy::too_many_arguments)]
fn spawn_task<E, X>(
    task: Task<E, X>,
    tracked_slot: Option<CancelId>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &ResourceMap,
    activity: &Activity,
    active_tasks: &ActiveTasks,
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
            let resources = resources.clone();
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let token = tracked_slot.map(|_| next_task_token());
            let cleanup_slot = tracked_slot;
            let cleanup_token = token;
            activity.inc();

            let handle = compio::runtime::spawn(async move {
                let _activity_guard = ActivityGuard::new(activity.clone());
                let command = future.await;
                route_spawned_command(
                    command,
                    &event_tx,
                    &effect_handler,
                    &resources,
                    &activity,
                    &active_tasks,
                );

                if let (Some(id), Some(token)) = (cleanup_slot, cleanup_token) {
                    cleanup_tracked_slot_if_current(&active_tasks, id, token);
                }
            });

            Some(SpawnedTask { token, handle })
        }
        Task::Stream(stream) => {
            let has_runtime = compio::runtime::Runtime::try_with_current(|_| ()).is_ok();
            if !has_runtime {
                let commands = futures::executor::block_on(stream.collect::<Vec<_>>());
                for command in commands {
                    queue.push_back(command);
                }
                return None;
            }

            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = resources.clone();
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            let token = tracked_slot.map(|_| next_task_token());
            let cleanup_slot = tracked_slot;
            let cleanup_token = token;
            activity.inc();

            let handle = compio::runtime::spawn(async move {
                let _activity_guard = ActivityGuard::new(activity.clone());
                futures::pin_mut!(stream);
                while let Some(command) = stream.next().await {
                    route_spawned_command(
                        command,
                        &event_tx,
                        &effect_handler,
                        &resources,
                        &activity,
                        &active_tasks,
                    );
                }

                if let (Some(id), Some(token)) = (cleanup_slot, cleanup_token) {
                    cleanup_tracked_slot_if_current(&active_tasks, id, token);
                }
            });

            Some(SpawnedTask { token, handle })
        }
    }
}

fn route_spawned_command<E, X>(
    command: Command<E, X>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &ResourceMap,
    activity: &Activity,
    active_tasks: &ActiveTasks,
) where
    E: 'static,
    X: 'static,
{
    let _ = route_command_iterative(
        command,
        event_tx,
        effect_handler,
        resources,
        activity,
        active_tasks,
        false,
    );
}
