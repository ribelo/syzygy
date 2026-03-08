use std::future::Future;
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
            if crate::runtime::try_with_current(|| ()).is_ok() {
                crate::runtime::await_blocking(crate::runtime::spawn_blocking(blocking)).await
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
        match self {
            Self::None => Task::None,
            Self::Resolved(command) => Task::Resolved(command.map(fe, fx)),
            _ => self.map_impl(namespace, Rc::new(fe), Rc::new(fx)),
        }
    }

    fn map_impl<Namespace, E2, X2, FE, FX>(
        self,
        namespace: Namespace,
        fe: Rc<FE>,
        fx: Rc<FX>,
    ) -> Task<E2, X2>
    where
        Namespace: Clone + 'static,
        FE: Fn(E) -> E2 + 'static,
        FX: Fn(X) -> X2 + 'static,
        E2: 'static,
        X2: 'static,
    {
        match self {
            Self::None => Task::None,
            Self::Resolved(command) => {
                Task::Resolved(command.map(|event| fe(event), |effect| fx(effect)))
            }
            Self::Future(future) => {
                let fe = Rc::clone(&fe);
                let fx = Rc::clone(&fx);
                Task::Future(Box::pin(async move {
                    let command = future.await;
                    let _ = namespace;
                    command.map(|event| fe(event), |effect| fx(effect))
                }))
            }
            Self::Stream(stream) => {
                let fe = Rc::clone(&fe);
                let fx = Rc::clone(&fx);
                Task::Stream(Box::pin(stream.map(move |command| {
                    let _ = namespace.clone();
                    command.map(|event| fe(event), |effect| fx(effect))
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
    fn map_keeps_abortable_task_and_command_leases_aligned() {
        let lease = crate::command::TaskLease::new();
        let abortable = Command::<ChildEvent, ChildEffect>::abortable(&lease, ChildEffect)
            .map(ParentEvent::Child, ParentEffect::Child)
            .into_iter()
            .collect::<Vec<_>>();

        let cancel_task = Task::<ChildEvent, ChildEffect>::resolved(Command::cancel(&lease))
            .map(ParentEvent::Child, ParentEffect::Child);

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
    fn map_future_keeps_abortable_commands() {
        let lease = crate::command::TaskLease::new();
        let mapped = Task::<ChildEvent, ChildEffect>::once(async move { Command::cancel(&lease) })
            .map(ParentEvent::Child, ParentEffect::Child);

        let command = match mapped {
            Task::Future(future) => futures::executor::block_on(future),
            _ => panic!("expected future task"),
        };

        let steps = command.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0], CommandStep::Cancel { .. }));
    }

    #[test]
    fn map_stream_keeps_abortable_commands() {
        let lease = crate::command::TaskLease::new();
        let mapped =
            Task::<ChildEvent, ChildEffect>::stream(futures::stream::iter([Command::cancel(
                &lease,
            )]))
            .map(ParentEvent::Child, ParentEffect::Child);

        let commands = match mapped {
            Task::Stream(stream) => futures::executor::block_on(stream.collect::<Vec<_>>()),
            _ => panic!("expected stream task"),
        };

        assert_eq!(commands.len(), 1);
        let steps = commands[0].clone().into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);
        assert!(matches!(steps[0], CommandStep::Cancel { .. }));
    }
}
