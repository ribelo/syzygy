pub mod context;
pub mod dispatch;
pub mod model;
pub mod reactive;
pub mod resource;
pub mod syzygy;

pub mod prelude {
    #[cfg(feature = "async")]
    pub use crate::context::r#async::AsyncContext;
    pub use crate::context::{Context, FromContext, IntoContext};
    pub use crate::dispatch::DispatchEffect;
    pub use crate::model::{ModelAccess, ModelModify, ModelSnapshotAccess};
    pub use crate::resource::{ResourceAccess, ResourceModify, Resources};
    // Note: parallel features not implemented yet
    // #[cfg(feature = "parallel")]
    // pub use crate::spawn::{RayonPool, SpawnParallel};
    pub use crate::syzygy::Syzygy;
}
