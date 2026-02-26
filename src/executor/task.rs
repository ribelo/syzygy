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
    pub fn map<E2, X2>(
        self,
        fe: impl Fn(E) -> E2 + 'static,
        fx: impl Fn(X) -> X2 + 'static,
    ) -> Task<E2, X2>
    where
        E2: 'static,
        X2: 'static,
    {
        let fe = Rc::new(fe);
        let fx = Rc::new(fx);

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
                    command.map(|event| fe(event), |effect| fx(effect))
                }))
            }
            Self::Stream(stream) => {
                let fe = Rc::clone(&fe);
                let fx = Rc::clone(&fx);
                Task::Stream(Box::pin(stream.map(move |command| {
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
