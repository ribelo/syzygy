#[cfg(feature = "parallel")]
use std::sync::Arc;

use crate::{
    dispatch::{DispatchEffect, Dispatcher},
    event_bus::{EmitEvent, EventBus},
    resource::{ResourceAccess, Resources},
};

#[cfg(feature = "parallel")]
use crate::spawn::SpawnParallel;
#[cfg(not(feature = "parallel"))]
use crate::spawn::SpawnThread;

use super::{Context, FromContext};

#[derive(Clone)]
pub struct ThreadContext {
    resources: Resources,
    dispatcher: Dispatcher,
    event_bus: EventBus,
    #[cfg(feature = "parallel")]
    rayon_pool: Arc<rayon::ThreadPool>,
}

impl Context for ThreadContext {}

#[cfg(not(feature = "parallel"))]
impl<C> FromContext<C> for ThreadContext
where
    C: ResourceAccess + DispatchEffect + EmitEvent + SpawnThread + 'static,
{
    fn from_context(cx: &C) -> Self {
        Self {
            dispatcher: <C as DispatchEffect>::dispatcher(cx).clone(),
            resources: <C as ResourceAccess>::resources(cx).clone(),
            event_bus: <C as EmitEvent>::event_bus(cx).clone(),
        }
    }
}

#[cfg(feature = "parallel")]
impl<C> FromContext<C> for ThreadContext
where
    C: ResourceAccess + DispatchEffect + EmitEvent + SpawnParallel + 'static,
{
    fn from_context(cx: &C) -> Self {
        Self {
            dispatcher: <C as DispatchEffect>::dispatcher(cx).clone(),
            resources: <C as ResourceAccess>::resources(cx).clone(),
            event_bus: <C as EmitEvent>::event_bus(cx).clone(),
            rayon_pool: <C as SpawnParallel>::rayon_pool(cx),
        }
    }
}

impl ResourceAccess for ThreadContext {
    fn resources(&self) -> &Resources {
        &self.resources
    }
}

impl DispatchEffect for ThreadContext {
    fn dispatcher(&self) -> &Dispatcher {
        &self.dispatcher
    }
}

impl EmitEvent for ThreadContext {
    fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }
}

#[cfg(feature = "parallel")]
impl SpawnParallel for ThreadContext {
    fn rayon_pool(&self) -> Arc<rayon::ThreadPool> {
        Arc::clone(&self.rayon_pool)
    }
}
