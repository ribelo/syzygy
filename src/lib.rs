pub mod context;
pub mod debug;
pub mod dispatch;
pub mod effect_builder;
pub mod error;
pub mod event;
pub mod model;
pub mod reactive;
pub mod resource;
pub mod state;
pub mod syzygy;

pub mod prelude {
    pub use crate::context::Context;
    #[cfg(feature = "async")]
    pub use crate::context::snapshot::SnapshotContext;
    pub use crate::debug::{
        EffectTracer, SyzygyMetrics, disable_tracing, enable_metrics, enable_tracing, metrics,
        print_debug_summary, with_tracer,
    };
    #[cfg(feature = "async")]
    pub use crate::dispatch::AsyncDispatchExt;
    pub use crate::dispatch::{DispatchEffect, Effect};
    pub use crate::event::Event;
    pub use crate::effect_builder::{EffectBuilder, EffectExt};
    pub use crate::error::{
        ContextCreationError, DispatchError, LockAcquisitionError, ResourceNotFoundError,
        ResourceReplaceError,
    };
    pub use crate::model::{Model, ModelAccess, ModelModify, ModelSnapshotAccess};
    pub use crate::resource::{ResourceAccess, ResourceModify, Resources};
    pub use crate::state::{StateAccess, StateModify};
    // Note: parallel features not implemented yet
    // #[cfg(feature = "parallel")]
    // pub use crate::spawn::{RayonPool, SpawnParallel};
    pub use crate::syzygy::Syzygy;
}
