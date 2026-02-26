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

    pub fn send(&self, event: E) -> Result<(), CoreError> {
        self.try_send_owned(event).map_err(map_send_error)
    }

    pub fn try_send(&self, event: E) -> Result<(), CoreError> {
        self.send(event)
    }

    pub fn try_send_event(&self, event: E) -> Result<(), CoreError> {
        self.send(event)
    }
}

fn map_send_error<E>(error: TrySendError<E>) -> CoreError {
    match error {
        TrySendError::Full(_) => CoreError::ChannelFull,
        TrySendError::Disconnected(_) => CoreError::ChannelClosed,
    }
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
    command_buffer: Vec<Command<E, X>>,
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
        Self::with_event_channel_capacity(event_handler, model, None)
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
            command_buffer: Vec::new(),
            event_rx: rx,
            event_tx: event_tx.clone(),
            event_channel_capacity: capacity,
        };

        (core, event_tx)
    }

    pub fn handle_event(&mut self, event: E) -> Command<E, X> {
        let ctx = EventContext::new(&mut self.model);
        (self.event_handler)(event, &ctx)
    }

    pub fn process_events(&mut self) -> Vec<Command<E, X>> {
        while let Ok(event) = self.event_rx.try_recv() {
            self.event_queue.push_back(event);
        }

        while let Some(event) = self.event_queue.pop_front() {
            let command = self.handle_event(event);
            self.command_buffer.push(command);
        }

        std::mem::take(&mut self.command_buffer)
    }

    #[deprecated(note = "use try_send_event()")]
    pub fn send_event(&self, event: E) {
        let _ = self.try_send_event(event);
    }

    pub fn try_send_event(&self, event: E) -> Result<(), CoreError> {
        self.event_tx.try_send_event(event)
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
