use std::sync::Arc;

use crate::{
    dispatch::EffectsTx,
    model::{Model, ModelSnapshotAccess, ModelSnapshotCreate},
    prelude::DispatchEffect,
    resource::{ResourceAccess, Resources},
    syzygy::Syzygy,
};

use bon::Builder;

use super::Context;

/// Context for snapshot-based operations
///
/// `SnapshotContext` provides a snapshot-based view of the application state
/// that can be safely shared across async boundaries and threads. It contains
/// an immutable snapshot of the model and shared access to resources.
///
/// This is particularly useful for:
/// - Async operations that need consistent state
/// - Background tasks that should not block the main thread
/// - Read-only operations that can work with point-in-time data
///
/// # Examples
///
/// ```rust
/// use syzygy::prelude::*;
///
/// # #[derive(Debug, Clone)]
/// # struct AppModel { counter: i32 }
/// # impl Model for AppModel {
/// #     type Snapshot = Self;
/// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// # }
/// # #[tokio::main]
/// # async fn main() {
/// let syzygy: Syzygy<AppModel> = Syzygy::builder()
///     .model(AppModel { counter: 42 })
///     .build();
///
/// // Create a snapshot context
/// let snapshot_ctx: SnapshotContext<AppModel> = SnapshotContext::from(&syzygy);
///
/// // Use in async operations
/// tokio::spawn(async move {
///     let count = snapshot_ctx.snapshot().counter;
///     println!("Count in async task: {}", count);
/// });
/// # }
/// ```
#[derive(Debug, Builder)]
pub struct SnapshotContext<M: Model, E = ()> {
    model_snapshot: Arc<M::Snapshot>,
    resources: Resources,
    effects_tx: EffectsTx<M, E>,
}

impl<M: Model, E> Context for SnapshotContext<M, E> {
    type Model = M;
    type Event = E;
}

impl<M: Model, E> Clone for SnapshotContext<M, E> {
    fn clone(&self) -> Self {
        Self {
            model_snapshot: Arc::clone(&self.model_snapshot),
            resources: self.resources.clone(),
            effects_tx: self.effects_tx.clone(),
        }
    }
}

impl<M: Model, E> From<&Syzygy<M, E>> for SnapshotContext<M, E> {
    fn from(context: &Syzygy<M, E>) -> Self {
        Self {
            model_snapshot: Arc::new(context.model.to_snapshot()),
            resources: context.resources().clone(),
            effects_tx: context.effects_bus.tx.clone(),
        }
    }
}

impl<M: Model, E> From<&mut Syzygy<M, E>> for SnapshotContext<M, E> {
    fn from(context: &mut Syzygy<M, E>) -> Self {
        Self {
            model_snapshot: Arc::new(context.model.to_snapshot()),
            resources: context.resources().clone(),
            effects_tx: context.effects_bus.tx.clone(),
        }
    }
}

impl<M: Model, E> ModelSnapshotAccess for SnapshotContext<M, E> {
    fn snapshot(&self) -> &<<Self as Context>::Model as Model>::Snapshot {
        &self.model_snapshot
    }
}

impl<M: Model, E> ModelSnapshotCreate for SnapshotContext<M, E> {
    fn create_snapshot(&self) -> <M as Model>::Snapshot {
        (*self.model_snapshot).clone()
    }
}

impl<M: Model, E> ResourceAccess for SnapshotContext<M, E> {
    fn resources(&self) -> &Resources {
        &self.resources
    }
}

impl<M: Model, E> DispatchEffect for SnapshotContext<M, E> {
    fn effects_tx(&self) -> &EffectsTx<M, E> {
        &self.effects_tx
    }
}

impl<M: Model, E> SnapshotContext<M, E> {

    /// Dispatch a closure-based effect from snapshot context
    pub fn dispatch_closure<F>(&self, f: F)
    where
        F: FnOnce(&mut crate::syzygy::Syzygy<M, E>) + Send + 'static,
    {
        use crate::event::Message;
        self.effects_tx.send(Message::Closure(Box::new(f)))
            .expect("Effect receiver should be active");
    }
}
