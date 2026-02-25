use std::rc::Rc;

use futures::stream::StreamExt;

use crate::activity::Activity;
use crate::command::{Command, CommandStep};
use crate::core::EventSender;
use crate::dependency::ResourceMap;
use crate::error::ShellError;
use crate::executor::Task;
use crate::extract::EffectContext;

pub(crate) type EffectHandlerFn<E, X> = Rc<dyn Fn(X, &EffectContext) -> Task<E, X>>;

pub struct Shell<E, X>
where
    E: 'static,
    X: 'static,
{
    event_tx: EventSender<E>,
    effect_handler: EffectHandlerFn<E, X>,
    resources: ResourceMap,
    activity: Activity,
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
                self.run_effect(effect);
            }
            CommandStep::Batch(effects) | CommandStep::Parallel(effects) => {
                for effect in effects {
                    self.run_effect(effect);
                }
            }
        }

        Ok(())
    }

    fn run_effect(&mut self, effect: X) {
        let ctx = EffectContext::new(self.resources.clone());
        let task = (self.effect_handler)(effect, &ctx);
        self.drive_task(task);
    }

    fn drive_task(&mut self, task: Task<E, X>) {
        match task {
            Task::None => {}
            Task::Resolved(command) => {
                let _ = self.route_command(command);
            }
            Task::Once(future) => {
                let event_tx = self.event_tx.clone();
                let effect_handler = Rc::clone(&self.effect_handler);
                let resources = self.resources.clone();
                let activity = self.activity.clone();
                activity.inc();

                compio::runtime::spawn(async move {
                    let command = future.await;
                    route_spawned_command(
                        command,
                        &event_tx,
                        &effect_handler,
                        &resources,
                        &activity,
                    );
                    activity.dec();
                })
                .detach();
            }
            Task::Stream(stream) => {
                let event_tx = self.event_tx.clone();
                let effect_handler = Rc::clone(&self.effect_handler);
                let resources = self.resources.clone();
                let activity = self.activity.clone();
                activity.inc();

                compio::runtime::spawn(async move {
                    futures::pin_mut!(stream);
                    while let Some(command) = stream.next().await {
                        route_spawned_command(
                            command,
                            &event_tx,
                            &effect_handler,
                            &resources,
                            &activity,
                        );
                    }
                    activity.dec();
                })
                .detach();
            }
        }
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
    }

    pub fn wait_for_executors(&self) {}
}

fn route_spawned_command<E, X>(
    command: Command<E, X>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &ResourceMap,
    activity: &Activity,
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
                drive_spawned_effect(effect, event_tx, effect_handler, resources, activity);
            }
            CommandStep::Batch(effects) | CommandStep::Parallel(effects) => {
                for effect in effects {
                    drive_spawned_effect(effect, event_tx, effect_handler, resources, activity);
                }
            }
        }
    }
}

fn drive_spawned_effect<E, X>(
    effect: X,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &ResourceMap,
    activity: &Activity,
) where
    E: 'static,
    X: 'static,
{
    let ctx = EffectContext::new(resources.clone());
    match effect_handler(effect, &ctx) {
        Task::None => {}
        Task::Resolved(command) => {
            route_spawned_command(command, event_tx, effect_handler, resources, activity);
        }
        Task::Once(future) => {
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = resources.clone();
            let activity = activity.clone();
            activity.inc();

            compio::runtime::spawn(async move {
                let command = future.await;
                route_spawned_command(command, &event_tx, &effect_handler, &resources, &activity);
                activity.dec();
            })
            .detach();
        }
        Task::Stream(stream) => {
            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = resources.clone();
            let activity = activity.clone();
            activity.inc();

            compio::runtime::spawn(async move {
                futures::pin_mut!(stream);
                while let Some(command) = stream.next().await {
                    route_spawned_command(
                        command,
                        &event_tx,
                        &effect_handler,
                        &resources,
                        &activity,
                    );
                }
                activity.dec();
            })
            .detach();
        }
    }
}
