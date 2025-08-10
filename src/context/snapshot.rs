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
/// ## Important Architecture Principle
///
/// **SnapshotContext can only dispatch events, it cannot spawn tasks.**
/// This enforces the principle that "events control flow, tasks execute work".
/// Only Syzygy itself can spawn tasks, ensuring all control flow is visible
/// in event handlers.
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
/// # #[derive(Debug)]
/// # enum MyEvent { DataProcessed(i32) }
/// # impl Event<AppModel> for MyEvent {
/// #     fn apply(self, _ctx: &mut Syzygy<AppModel, Self>) {}
/// # }
/// # #[tokio::main]
/// # async fn main() {
/// let mut syzygy: Syzygy<AppModel, MyEvent> = Syzygy::builder()
///     .model(AppModel { counter: 42 })
///     .build();
///
/// // Tasks receive a snapshot context
/// syzygy.task(|snapshot_ctx| async move {
///     let count = snapshot_ctx.snapshot().counter;
///     
///     // Tasks can only dispatch events back, not spawn more tasks
///     snapshot_ctx.dispatch(MyEvent::DataProcessed(count));
///     
///     // This would NOT compile - SnapshotContext cannot spawn tasks:
///     // snapshot_ctx.task(|_| async {}); // ❌ Not allowed!
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
