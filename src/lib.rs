pub mod context;
pub mod debug;
pub mod dispatch;
pub mod effect_builder;
pub mod error;
pub mod model;
pub mod reactive;
pub mod resource;
pub mod state;
pub mod syzygy;

pub mod prelude {
    #[cfg(feature = "async")]
    pub use crate::context::r#async::AsyncContext;
    pub use crate::context::Context;
    pub use crate::dispatch::{DispatchEffect, Effect};
    pub use crate::effect_builder::{EffectBuilder, EffectExt};
    pub use crate::model::{ModelAccess, ModelModify, ModelSnapshotAccess};
    pub use crate::resource::{ResourceAccess, ResourceModify, Resources};
    pub use crate::state::{StateAccess, StateModify};
    pub use crate::error::{ResourceNotFoundError, DispatchError, ContextCreationError, LockAcquisitionError, ResourceReplaceError};
    pub use crate::debug::{EffectTracer, SyzygyMetrics, enable_tracing, disable_tracing, enable_metrics, print_debug_summary, with_tracer, metrics};
    // Note: parallel features not implemented yet
    // #[cfg(feature = "parallel")]
    // pub use crate::spawn::{RayonPool, SpawnParallel};
    pub use crate::syzygy::Syzygy;
}
