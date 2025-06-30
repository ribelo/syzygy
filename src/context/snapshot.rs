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
/// let syzygy = Syzygy::builder()
///     .model(AppModel { counter: 42 })
///     .build();
///
/// // Create a snapshot context
/// let snapshot_ctx = SnapshotContext::from(&syzygy);
///
/// // Use in async operations
/// tokio::spawn(async move {
///     let count = snapshot_ctx.snapshot().counter;
///     println!("Count in async task: {}", count);
/// });
/// # }
/// ```
#[derive(Debug, Builder)]
pub struct SnapshotContext<M: Model> {
    model_snapshot: Arc<M::Snapshot>,
    resources: Resources,
    effects_tx: EffectsTx<M>,
}

impl<M: Model> Context for SnapshotContext<M> {
    type Model = M;
}

impl<M: Model> Clone for SnapshotContext<M> {
    fn clone(&self) -> Self {
        Self {
            model_snapshot: Arc::clone(&self.model_snapshot),
            resources: self.resources.clone(),
            effects_tx: self.effects_tx.clone(),
        }
    }
}

impl<M: Model> From<&Syzygy<M>> for SnapshotContext<M> {
    fn from(context: &Syzygy<M>) -> Self {
        Self {
            model_snapshot: Arc::new(context.model.to_snapshot()),
            resources: context.resources().clone(),
            effects_tx: context.effects_bus.tx.clone(),
        }
    }
}

impl<M: Model> From<&mut Syzygy<M>> for SnapshotContext<M> {
    fn from(context: &mut Syzygy<M>) -> Self {
        Self {
            model_snapshot: Arc::new(context.model.to_snapshot()),
            resources: context.resources().clone(),
            effects_tx: context.effects_bus.tx.clone(),
        }
    }
}

impl<M: Model> ModelSnapshotAccess for SnapshotContext<M> {
    fn snapshot(&self) -> &<<Self as Context>::Model as Model>::Snapshot {
        &self.model_snapshot
    }
}

impl<M: Model> ModelSnapshotCreate for SnapshotContext<M> {
    fn create_snapshot(&self) -> <M as Model>::Snapshot {
        (*self.model_snapshot).clone()
    }
}

impl<M: Model> ResourceAccess for SnapshotContext<M> {
    fn resources(&self) -> &Resources {
        &self.resources
    }
}

impl<M: Model> DispatchEffect for SnapshotContext<M> {
    fn effects_tx(&self) -> &EffectsTx<M> {
        &self.effects_tx
    }
}
