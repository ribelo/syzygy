use std::collections::HashMap;
use std::rc::Rc;

use futures::future::{AbortHandle, Abortable};
use futures::stream::StreamExt;

use crate::activity::Activity;
use crate::command::{CancelId, Command, CommandStep};
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
    cancel_handles: Rc<std::cell::RefCell<HashMap<CancelId, AbortHandle>>>,
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
            cancel_handles: Rc::new(std::cell::RefCell::new(HashMap::new())),
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
                self.run_effect(effect, None);
            }
            CommandStep::CancellableEffect { id, effect, .. } => {
                if let Some(handle) = self.cancel_handles.borrow_mut().remove(&id) {
                    handle.abort();
                }
                self.run_effect(effect, Some(id));
            }
            CommandStep::Cancel(id) => {
                if let Some(handle) = self.cancel_handles.borrow_mut().remove(&id) {
                    handle.abort();
                }
            }
            CommandStep::Batch(effects) | CommandStep::Parallel(effects) => {
                for effect in effects {
                    self.run_effect(effect, None);
                }
            }
        }

        Ok(())
    }

    fn run_effect(&mut self, effect: X, cancel_id: Option<CancelId>) {
        let ctx = EffectContext::new(self.resources.clone());
        let task = (self.effect_handler)(effect, &ctx);
        self.drive_task(task, cancel_id);
    }

    fn drive_task(&mut self, task: Task<E, X>, cancel_id: Option<CancelId>) {
        match task {
            Task::None => {}
            Task::Resolved(command) => {
                let _ = self.route_command(command);
            }
            Task::Once(future) => {
                let (abort_handle, abort_registration) = AbortHandle::new_pair();
                if let Some(id) = cancel_id {
                    self.cancel_handles.borrow_mut().insert(id, abort_handle);
                }

                let event_tx = self.event_tx.clone();
                let effect_handler = Rc::clone(&self.effect_handler);
                let resources = self.resources.clone();
                let cancel_handles = Rc::clone(&self.cancel_handles);
                let activity = self.activity.clone();
                activity.inc();

                compio::runtime::spawn(async move {
                    let result = Abortable::new(future, abort_registration).await;
                    if let Ok(command) = result {
                        route_spawned_command(
                            command,
                            &event_tx,
                            &effect_handler,
                            &resources,
                            &cancel_handles,
                            &activity,
                        );
                    }
                    activity.dec();
                })
                .detach();
            }
            Task::Stream(stream) => {
                let (abort_handle, abort_registration) = AbortHandle::new_pair();
                if let Some(id) = cancel_id {
                    self.cancel_handles.borrow_mut().insert(id, abort_handle);
                }

                let event_tx = self.event_tx.clone();
                let effect_handler = Rc::clone(&self.effect_handler);
                let resources = self.resources.clone();
                let cancel_handles = Rc::clone(&self.cancel_handles);
                let activity = self.activity.clone();
                activity.inc();

                compio::runtime::spawn(async move {
                    let route_activity = activity.clone();
                    let output = Abortable::new(
                        async move {
                            futures::pin_mut!(stream);
                            while let Some(command) = stream.next().await {
                                route_spawned_command(
                                    command,
                                    &event_tx,
                                    &effect_handler,
                                    &resources,
                                    &cancel_handles,
                                    &route_activity,
                                );
                            }
                        },
                        abort_registration,
                    )
                    .await;

                    let _ = output;
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
        for (_, handle) in self.cancel_handles.borrow_mut().drain() {
            handle.abort();
        }
    }

    pub fn wait_for_executors(&self) {}
}

fn route_spawned_command<E, X>(
    command: Command<E, X>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &ResourceMap,
    cancel_handles: &Rc<std::cell::RefCell<HashMap<CancelId, AbortHandle>>>,
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
                drive_spawned_effect(
                    effect,
                    None,
                    event_tx,
                    effect_handler,
                    resources,
                    cancel_handles,
                    activity,
                );
            }
            CommandStep::CancellableEffect { id, effect, .. } => {
                if let Some(handle) = cancel_handles.borrow_mut().remove(&id) {
                    handle.abort();
                }
                drive_spawned_effect(
                    effect,
                    Some(id),
                    event_tx,
                    effect_handler,
                    resources,
                    cancel_handles,
                    activity,
                );
            }
            CommandStep::Cancel(id) => {
                if let Some(handle) = cancel_handles.borrow_mut().remove(&id) {
                    handle.abort();
                }
            }
            CommandStep::Batch(effects) | CommandStep::Parallel(effects) => {
                for effect in effects {
                    drive_spawned_effect(
                        effect,
                        None,
                        event_tx,
                        effect_handler,
                        resources,
                        cancel_handles,
                        activity,
                    );
                }
            }
        }
    }
}

fn drive_spawned_effect<E, X>(
    effect: X,
    cancel_id: Option<CancelId>,
    event_tx: &EventSender<E>,
    effect_handler: &EffectHandlerFn<E, X>,
    resources: &ResourceMap,
    cancel_handles: &Rc<std::cell::RefCell<HashMap<CancelId, AbortHandle>>>,
    activity: &Activity,
) where
    E: 'static,
    X: 'static,
{
    let ctx = EffectContext::new(resources.clone());
    match effect_handler(effect, &ctx) {
        Task::None => {}
        Task::Resolved(command) => {
            route_spawned_command(
                command,
                event_tx,
                effect_handler,
                resources,
                cancel_handles,
                activity,
            );
        }
        Task::Once(future) => {
            let (abort_handle, abort_registration) = AbortHandle::new_pair();
            if let Some(id) = cancel_id {
                cancel_handles.borrow_mut().insert(id, abort_handle);
            }

            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = resources.clone();
            let cancel_handles = Rc::clone(cancel_handles);
            let activity = activity.clone();
            activity.inc();

            compio::runtime::spawn(async move {
                let result = Abortable::new(future, abort_registration).await;
                if let Ok(command) = result {
                    route_spawned_command(
                        command,
                        &event_tx,
                        &effect_handler,
                        &resources,
                        &cancel_handles,
                        &activity,
                    );
                }
                activity.dec();
            })
            .detach();
        }
        Task::Stream(stream) => {
            let (abort_handle, abort_registration) = AbortHandle::new_pair();
            if let Some(id) = cancel_id {
                cancel_handles.borrow_mut().insert(id, abort_handle);
            }

            let event_tx = event_tx.clone();
            let effect_handler = Rc::clone(effect_handler);
            let resources = resources.clone();
            let cancel_handles = Rc::clone(cancel_handles);
            let activity = activity.clone();
            activity.inc();

            compio::runtime::spawn(async move {
                let route_activity = activity.clone();
                let output = Abortable::new(
                    async move {
                        futures::pin_mut!(stream);
                        while let Some(command) = stream.next().await {
                            route_spawned_command(
                                command,
                                &event_tx,
                                &effect_handler,
                                &resources,
                                &cancel_handles,
                                &route_activity,
                            );
                        }
                    },
                    abort_registration,
                )
                .await;

                let _ = output;
                activity.dec();
            })
            .detach();
        }
    }
}
