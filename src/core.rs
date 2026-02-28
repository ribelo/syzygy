use std::collections::VecDeque;
use std::time::Duration;

use crossbeam_channel::{bounded, unbounded, Receiver, RecvTimeoutError, Sender, TrySendError};

use crate::command::Command;
use crate::error::CoreError;
use crate::extract::EventContext;

pub(crate) type EventHandlerFn<E, X, M> = Box<dyn Fn(E, &EventContext<M>) -> Command<E, X>>;

/// Multi-producer sender returned by [`Core::new`].
pub struct EventSender<E> {
    inner: Sender<E>,
}

impl<E> Clone for EventSender<E> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<E> EventSender<E> {
    fn new(inner: Sender<E>) -> Self {
        Self { inner }
    }

    pub(crate) fn try_send_owned(&self, event: E) -> Result<(), TrySendError<E>> {
        self.inner.try_send(event)
    }

    #[must_use]
    pub(crate) fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn try_send(&self, event: E) -> Result<(), CoreError> {
        self.try_send_owned(event).map_err(map_send_error)
    }

    pub fn emit(&self, event: E) {
        if let Err(error) = self.try_send(event) {
            report_emit_error(error);
        }
    }
}

fn map_send_error<E>(error: TrySendError<E>) -> CoreError {
    match error {
        TrySendError::Full(_) => CoreError::ChannelFull,
        TrySendError::Disconnected(_) => CoreError::ChannelClosed,
    }
}

fn report_emit_error(error: CoreError) {
    match error {
        CoreError::ChannelClosed => report_emit_disconnected(),
        CoreError::ChannelFull => report_emit_full(),
    }
}

fn report_emit_disconnected() {
    #[cfg(feature = "tracing")]
    tracing::debug!("dropping emitted event because event channel is disconnected");
}

fn report_emit_full() {
    #[cfg(feature = "tracing")]
    tracing::warn!("dropping emitted event because event channel is full");
}

/// Core handles synchronous event processing and owns the model.
pub struct Core<E, X, M>
where
    E: 'static,
    X: 'static,
{
    event_handler: EventHandlerFn<E, X, M>,
    model: M,
    event_queue: VecDeque<E>,
    event_rx: Receiver<E>,
    event_tx: EventSender<E>,
    event_channel_capacity: Option<usize>,
}

impl<E, X, M> Core<E, X, M>
where
    E: 'static,
    X: 'static,
{
    pub fn new(event_handler: EventHandlerFn<E, X, M>, model: M) -> (Self, EventSender<E>) {
        Self::with_event_channel_capacity(event_handler, model, Some(128))
    }

    pub fn with_event_channel_capacity(
        event_handler: EventHandlerFn<E, X, M>,
        model: M,
        capacity: Option<usize>,
    ) -> (Self, EventSender<E>) {
        let (tx, rx) = match capacity {
            Some(capacity) => bounded(capacity),
            None => unbounded(),
        };
        let event_tx = EventSender::new(tx);
        let core = Self {
            event_handler,
            model,
            event_queue: VecDeque::new(),
            event_rx: rx,
            event_tx: event_tx.clone(),
            event_channel_capacity: capacity,
        };

        (core, event_tx)
    }

    #[must_use]
    pub fn handle_event(&mut self, event: E) -> Command<E, X> {
        let ctx = EventContext::new(&mut self.model);
        (self.event_handler)(event, &ctx)
    }

    #[must_use]
    pub fn process_events(&mut self) -> Vec<Command<E, X>> {
        let mut commands = Vec::new();
        self.process_events_into(|command| commands.push(command));
        commands
    }

    pub fn process_events_try_into<F, Error>(&mut self, mut sink: F) -> Result<usize, Error>
    where
        F: FnMut(Command<E, X>) -> Result<(), Error>,
    {
        while let Ok(event) = self.event_rx.try_recv() {
            self.event_queue.push_back(event);
        }

        let mut emitted = 0usize;
        while let Some(event) = self.event_queue.pop_front() {
            let command = self.handle_event(event);
            sink(command)?;
            emitted = emitted.saturating_add(1);
        }

        Ok(emitted)
    }

    pub fn process_events_into<F>(&mut self, mut sink: F) -> usize
    where
        F: FnMut(Command<E, X>),
    {
        self.process_events_try_into(|command| {
            sink(command);
            Ok::<(), ()>(())
        })
        .expect("infallible command sink")
    }

    pub fn try_send(&self, event: E) -> Result<(), CoreError> {
        self.event_tx.try_send(event)
    }

    pub fn emit(&self, event: E) {
        self.event_tx.emit(event);
    }

    #[must_use]
    pub fn event_sender(&self) -> EventSender<E> {
        self.event_tx.clone()
    }

    #[must_use]
    pub fn event_channel_capacity(&self) -> Option<usize> {
        self.event_channel_capacity
    }

    #[must_use]
    pub fn model(&self) -> &M {
        &self.model
    }

    #[must_use]
    pub fn model_mut(&mut self) -> &mut M {
        &mut self.model
    }

    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.event_queue.len().saturating_add(self.event_rx.len())
    }

    #[must_use]
    pub fn has_pending_events(&self) -> bool {
        self.pending_count() > 0
    }

    #[must_use]
    pub fn wait_for_event(&mut self, timeout: Duration) -> bool {
        if !self.event_queue.is_empty() {
            return true;
        }

        match self.event_rx.recv_timeout(timeout) {
            Ok(event) => {
                self.event_queue.push_back(event);
                true
            }
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => false,
        }
    }
}

impl<E, X, M> std::fmt::Debug for Core<E, X, M>
where
    E: 'static,
    X: 'static,
    M: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core")
            .field("model", &self.model)
            .field("pending_events", &self.pending_count())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::Core;
    use crate::command::Command;
    use crate::extract::EventContext;

    #[test]
    fn process_events_try_into_stops_on_sink_error_and_preserves_pending_events() {
        let handled = Rc::new(Cell::new(0usize));
        let handled_in_handler = Rc::clone(&handled);
        let (mut core, tx) = Core::new(
            Box::new(move |_event: u8, _ctx: &EventContext<()>| {
                handled_in_handler.set(handled_in_handler.get().saturating_add(1));
                Command::<u8, ()>::none()
            }),
            (),
        );

        tx.emit(1);
        tx.emit(2);

        let result: Result<usize, ()> = core.process_events_try_into(|_command| Err(()));

        assert!(result.is_err());
        assert_eq!(handled.get(), 1);
        assert_eq!(core.pending_count(), 1);

        let _ = core.process_events();
        assert_eq!(handled.get(), 2);
    }
}
