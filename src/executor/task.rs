use std::future::Future;
use std::hash::Hash;
use std::pin::Pin;
use std::rc::Rc;

use futures::Stream;
use futures::StreamExt;

use crate::command::Command;

/// Declarative unit of work returned by effect handlers.
pub enum Task<E, X> {
    None,
    Resolved(Command<E, X>),
    Future(Pin<Box<dyn Future<Output = Command<E, X>> + 'static>>),
    Stream(Pin<Box<dyn Stream<Item = Command<E, X>> + 'static>>),
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
    pub fn blocking<F>(blocking: F) -> Self
    where
        F: FnOnce() -> Command<E, X> + Send + 'static,
        E: Send,
        X: Send,
    {
        Self::future(async move {
            if compio::runtime::Runtime::try_with_current(|_| ()).is_ok() {
                match compio::runtime::spawn_blocking(blocking).await {
                    Ok(command) => command,
                    Err(panic) => std::panic::resume_unwind(panic),
                }
            } else {
                blocking()
            }
        })
    }

    #[must_use]
    pub fn stream<S>(stream: S) -> Self
    where
        S: Stream<Item = Command<E, X>> + 'static,
    {
        Self::Stream(Box::pin(stream))
    }

    #[must_use]
    pub fn map<E2, X2, FE, FX>(self, fe: FE, fx: FX) -> Task<E2, X2>
    where
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        let namespace = (std::any::type_name::<FE>(), std::any::type_name::<FX>());
        self.map_impl(namespace, Rc::new(fe), Rc::new(fx), true)
    }

    #[must_use]
    pub fn map_namespaced<Namespace, E2, X2, FE, FX>(
        self,
        namespace: Namespace,
        fe: FE,
        fx: FX,
    ) -> Task<E2, X2>
    where
        Namespace: Hash + Clone + 'static,
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        self.map_impl(namespace, Rc::new(fe), Rc::new(fx), false)
    }

    fn map_impl<Namespace, E2, X2, FE, FX>(
        self,
        namespace: Namespace,
        fe: Rc<FE>,
        fx: Rc<FX>,
        reject_cancellable: bool,
    ) -> Task<E2, X2>
    where
        Namespace: Hash + Clone + 'static,
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        match self {
            Self::None => Task::None,
            Self::Resolved(command) => {
                assert!(
                    !(reject_cancellable && command.has_cancellable_steps()),
                    "Task::map cannot safely namespace tracked/cancel steps; use Task::map_namespaced(namespace, ...)"
                );
                Task::Resolved(command.map_namespaced(
                    namespace,
                    |event| fe(event),
                    |effect| fx(effect),
                ))
            }
            Self::Future(future) => {
                let fe = Rc::clone(&fe);
                let fx = Rc::clone(&fx);
                Task::Future(Box::pin(async move {
                    let command = future.await;
                    if reject_cancellable && command.has_cancellable_steps() {
                        report_map_namespace_violation();
                        return Command::none();
                    }

                    command.map_namespaced(namespace, |event| fe(event), |effect| fx(effect))
                }))
            }
            Self::Stream(stream) => {
                let fe = Rc::clone(&fe);
                let fx = Rc::clone(&fx);
                Task::Stream(Box::pin(stream.map(move |command| {
                    if reject_cancellable && command.has_cancellable_steps() {
                        report_map_namespace_violation();
                        return Command::none();
                    }

                    command.map_namespaced(
                        namespace.clone(),
                        |event| fe(event),
                        |effect| fx(effect),
                    )
                })))
            }
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

fn report_map_namespace_violation() {
    #[cfg(feature = "tracing")]
    tracing::error!(
        "Task::map dropped command with tracked/cancel steps; use Task::map_namespaced(namespace, ...)"
    );
}

#[cfg(test)]
mod tests {
    use futures::StreamExt;

    use super::Task;
    use crate::command::{Command, CommandStep};

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
    fn map_namespaced_keeps_task_and_command_cancel_ids_aligned() {
        let tracked = Command::<ChildEvent, ChildEffect>::track(1_u8, ChildEffect)
            .map_namespaced(9_u8, ParentEvent::Child, ParentEffect::Child)
            .into_iter()
            .collect::<Vec<_>>();

        let cancel_task = Task::<ChildEvent, ChildEffect>::resolved(Command::cancel(1_u8))
            .map_namespaced(9_u8, ParentEvent::Child, ParentEffect::Child);

        let cancel = match cancel_task {
            Task::Resolved(command) => command.into_iter().collect::<Vec<_>>(),
            _ => panic!("expected resolved task"),
        };

        let tracked_id = match &tracked[0] {
            CommandStep::Tracked { id, .. } => *id,
            _ => panic!("expected tracked command"),
        };

        let cancel_id = match &cancel[0] {
            CommandStep::Cancel { id } => *id,
            _ => panic!("expected cancel command"),
        };

        assert_eq!(tracked_id, cancel_id);
    }

    #[test]
    fn map_panics_for_cancellable_steps_without_namespace() {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = Task::<ChildEvent, ChildEffect>::resolved(Command::cancel(1_u8))
                .map(ParentEvent::Child, ParentEffect::Child);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn map_future_drops_cancellable_command_without_panicking() {
        let mapped = Task::<ChildEvent, ChildEffect>::once(async { Command::cancel(1_u8) })
            .map(ParentEvent::Child, ParentEffect::Child);

        let command = match mapped {
            Task::Future(future) => futures::executor::block_on(future),
            _ => panic!("expected future task"),
        };

        assert!(command.is_empty());
    }

    #[test]
    fn map_stream_drops_cancellable_commands_without_panicking() {
        let mapped =
            Task::<ChildEvent, ChildEffect>::stream(futures::stream::iter([Command::cancel(1_u8)]))
                .map(ParentEvent::Child, ParentEffect::Child);

        let commands = match mapped {
            Task::Stream(stream) => futures::executor::block_on(stream.collect::<Vec<_>>()),
            _ => panic!("expected stream task"),
        };

        assert_eq!(commands.len(), 1);
        assert!(commands[0].is_empty());
    }
}
