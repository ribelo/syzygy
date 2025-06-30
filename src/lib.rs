pub mod context;
pub mod dispatch;
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
    pub use crate::dispatch::DispatchEffect;
    pub use crate::model::{ModelAccess, ModelModify, ModelSnapshotAccess};
    pub use crate::resource::{ResourceAccess, ResourceModify, Resources};
    pub use crate::state::{StateAccess, StateModify};
    pub use crate::error::{ResourceNotFoundError, DispatchError, ContextCreationError, LockAcquisitionError, ResourceReplaceError};
    // Note: parallel features not implemented yet
    // #[cfg(feature = "parallel")]
    // pub use crate::spawn::{RayonPool, SpawnParallel};
    pub use crate::syzygy::Syzygy;
}
