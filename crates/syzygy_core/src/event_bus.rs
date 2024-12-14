use std::{
    any::TypeId,
    cell::RefCell,
    convert::Into,
    fmt,
    ops::{Deref, DerefMut},
    rc::Rc,
    sync::{Arc, RwLock},
};

use crossbeam_channel::{Receiver, Sender};
use downcast_rs::{impl_downcast, DowncastSync};
use rustc_hash::FxHashMap;
use thiserror::Error;

use crate::context::{event::EventContext, Context};

pub trait Event: DowncastSync + 'static {}
impl_downcast!(sync Event);

impl<T> Event for T where T: DowncastSync + 'static {}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct EventType {
    type_id: TypeId,
    pub(crate) name: String,
}

impl EventType {
    #[must_use]
    pub fn new<T: Event>() -> Self {
        Self {
            type_id: TypeId::of::<T>(),
            name: std::any::type_name::<T>().to_string(),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

type HandlerFn = Box<dyn Fn(&EventContext, &Box<dyn Event>) + Send + Sync>;

pub struct EventHandler {
    name: String,
    handler: HandlerFn,
}

#[derive(Debug, Clone)]
pub struct EventHandlers(Rc<RefCell<FxHashMap<EventType, Vec<EventHandler>>>>);

impl Deref for EventHandlers {
    type Target = Rc<RefCell<FxHashMap<EventType, Vec<EventHandler>>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for EventHandlers {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[allow(clippy::borrowed_box)]
impl EventHandler {
    pub fn handle(&self, cx: &EventContext, event: &Box<dyn Event>) {
        (self.handler)(cx, event);
    }
}

impl fmt::Debug for EventHandler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EventHandler")
            .field("name", &self.name)
            .field("handler", &"<handler>")
            .finish()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EventError {
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Runtime stopped")]
    RuntimeStopped,
}

#[derive(Debug, Clone)]
pub struct EventBus {
    pub(crate) tx: Sender<(EventType, Box<dyn Event>)>,
    pub(crate) rx: Receiver<(EventType, Box<dyn Event>)>,
    pub(crate) handlers: Arc<RwLock<FxHashMap<EventType, Vec<EventHandler>>>>,
}

impl Default for EventBus {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            tx,
            rx,
            handlers: Arc::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum EmitError {
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Runtime stopped")]
    RuntimeStopped,
}

#[derive(Debug, Error)]
pub enum EventHandlerError {
    #[error("Handler name '{0}' already exists")]
    AlreadyExists(String),
    #[error("Handler '{0}' not found")]
    HandlerNotFound(String),
    #[error("No event handlers registered for '{0}'")]
    Unregistered(String),
}

impl EventBus {
    pub fn emit<E>(&self, event: E) -> Result<(), EmitError>
    where
        E: Event,
    {
        self.tx
            .send((EventType::new::<E>(), Box::new(event)))
            .map_err(|_| EmitError::ChannelClosed)
    }

    pub fn subscribe<T>(
        &self,
        name: Option<impl Into<String>>,
        handler: impl Fn(&EventContext, &T) + Send + Sync + 'static,
    ) -> Result<(), EventHandlerError>
    where
        T: Event + Send + Sync + 'static,
    {
        let event_type = EventType::new::<T>();
        let mut handlers = self.handlers.write().unwrap();
        let name = name.map_or_else(|| std::any::type_name::<T>().to_string(), Into::into);

        // Check if handler name already exists
        if handlers
            .values()
            .any(|handlers| handlers.iter().any(|h| h.name == name))
        {
            return Err(EventHandlerError::AlreadyExists(name));
        }

        #[allow(clippy::borrowed_box)]
        let handler = Box::new(move |cx: &EventContext, event: &Box<dyn Event>| {
            if let Some(event) = event.downcast_ref::<T>() {
                handler(cx, event);
            }
        });

        handlers
            .entry(event_type)
            .or_default()
            .push(EventHandler { name, handler });

        Ok(())
    }

    pub fn unsubscribe(&self, name: impl Into<String>) -> Result<(), EventHandlerError> {
        let mut handlers = self.handlers.write().unwrap();
        let name = name.into();

        let mut found = false;
        for handlers_vec in handlers.values_mut() {
            let initial_len = handlers_vec.len();
            handlers_vec.retain(|handler| handler.name != name);
            if handlers_vec.len() < initial_len {
                found = true;
            }
        }

        if found {
            Ok(())
        } else {
            Err(EventHandlerError::HandlerNotFound(name))
        }
    }

    pub(crate) fn pop(&self) -> Option<(EventType, Box<dyn Event>)> {
        self.rx.try_recv().ok()
    }

    #[allow(clippy::needless_pass_by_value)]
    pub(crate) fn handle(
        &self,
        cx: &EventContext,
        event_type: &EventType,
        event: Box<dyn Event>,
    ) -> Result<(), EventHandlerError> {
        let handlers = self.handlers.read().unwrap();

        let handlers = handlers
            .get(event_type)
            .ok_or_else(|| EventHandlerError::Unregistered(event_type.name().to_string()))?;

        for handler in handlers {
            handler.handle(cx, &event);
        }

        Ok(())
    }
}

pub trait EmitEvent: Sized + Context {
    fn event_bus(&self) -> &EventBus;
    fn emit<E>(&self, event: E) -> Result<(), EmitError>
    where
        E: Event,
    {
        self.event_bus().emit(event)
    }
}

pub trait Subscribe: EmitEvent + Sized + Context {
    fn subscribe<E>(
        &self,
        name: Option<impl Into<String>>,
        handler: impl Fn(&EventContext, &E) + Send + Sync + 'static,
    ) -> Result<(), EventHandlerError>
    where
        E: Event + Send + Sync + 'static,
    {
        self.event_bus().subscribe(name, handler)
    }
}

pub trait Unsubscribe: Subscribe + Sized + Context {
    fn unsubscribe(&self, name: impl Into<String>) -> Result<(), EventHandlerError> {
        self.event_bus().unsubscribe(name)
    }
}
