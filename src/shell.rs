use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

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
type ActiveTasks = Rc<RefCell<HashMap<CancelId, TaskHandle>>>;

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

        self.route_command(command)
    }

    fn route_command(&mut self, command: Command<E, X>) -> Result<(), ShellError> {
        for step in command {
            self.route_step(step)?;
        }
        Ok(())
    }

    fn route_step(&mut self, step: CommandStep<E, X>) -> Result<(), ShellError> {
        match step {
            CommandStep::Event(event) => {
                self.event_tx
                    .send(event)
                    .map_err(|_| ShellError::EventChannelClosed)?;
            }
            CommandStep::Effect(effect) => {
                if let Some(handle) = self.run_effect(effect) {
                    handle.detach();
                }
            }
            CommandStep::Tracked { id, effect } => {
                cancel_tracked_slot(&self.active_tasks, id);
                if let Some(handle) = self.run_effect(effect) {
                    self.active_tasks.borrow_mut().insert(id, handle);
                }
            }
            CommandStep::Cancel { id } => {
                cancel_tracked_slot(&self.active_tasks, id);
            }
        }

        Ok(())
    }

    fn run_effect(&mut self, effect: X) -> Option<TaskHandle> {
        run_effect_task(
            effect,
            &self.event_tx,
            &self.effect_handler,
            &self.resources,
            &self.activity,
            &self.active_tasks,
        )
    }

    pub fn drain(&mut self) -> Result<usize, ShellError> {
        Ok(0)
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
    if let Some(task) = active_tasks.borrow_mut().remove(&id) {
        drop(task);
    }
}

fn run_effect_task<E, X>(
    effect: X,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &ResourceMap,
    activity: &Activity,
    active_tasks: &ActiveTasks,
) -> Option<TaskHandle>
where
    E: 'static,
    X: 'static,
{
    let ctx = EffectContext::new(resources.clone());
    let task = effect_handler(effect, &ctx);
    spawn_task(
        task,
        event_tx,
        effect_handler,
        resources,
        activity,
        active_tasks,
    )
}

fn spawn_task<E, X>(
    task: Task<E, X>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &ResourceMap,
    activity: &Activity,
    active_tasks: &ActiveTasks,
) -> Option<TaskHandle>
where
    E: 'static,
    X: 'static,
{
    match task {
        Task::None => None,
        Task::Resolved(command) => {
            route_spawned_command(
                command,
                event_tx,
                effect_handler,
                resources,
                activity,
                active_tasks,
            );
            None
        }
        Task::Future(future) => {
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = resources.clone();
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            activity.inc();

            Some(compio::runtime::spawn(async move {
                let command = future.await;
                route_spawned_command(
                    command,
                    &event_tx,
                    &effect_handler,
                    &resources,
                    &activity,
                    &active_tasks,
                );
                activity.dec();
            }))
        }
        Task::Stream(stream) => {
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = resources.clone();
            let activity = activity.clone();
            let active_tasks = Rc::clone(active_tasks);
            activity.inc();

            Some(compio::runtime::spawn(async move {
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
                activity.dec();
            }))
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
    for step in command {
        match step {
            CommandStep::Event(event) => {
                let _ = event_tx.send(event);
            }
            CommandStep::Effect(effect) => {
                if let Some(handle) = run_effect_task(
                    effect,
                    event_tx,
                    effect_handler,
                    resources,
                    activity,
                    active_tasks,
                ) {
                    handle.detach();
                }
            }
            CommandStep::Tracked { id, effect } => {
                cancel_tracked_slot(active_tasks, id);
                if let Some(handle) = run_effect_task(
                    effect,
                    event_tx,
                    effect_handler,
                    resources,
                    activity,
                    active_tasks,
                ) {
                    active_tasks.borrow_mut().insert(id, handle);
                }
            }
            CommandStep::Cancel { id } => {
                cancel_tracked_slot(active_tasks, id);
            }
        }
    }
}
