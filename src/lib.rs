pub mod builder;
pub mod codegen;
pub mod context;
pub mod dispatch;
pub mod error;
pub mod event_map;
pub mod extract;
pub mod handle;
pub mod handler;
pub mod indexed_map;
pub mod magic_handler;
pub mod model;
pub mod model_map;
pub mod recorder;
pub mod replay;
pub mod resource;
pub mod syzygy;
pub mod test_utils;

#[cfg(feature = "async")]
pub mod executor;

pub mod prelude {
    // THE Syzygy - Simple enum-based system
    pub use crate::builder::SyzygyBuilder;
    pub use crate::syzygy::Syzygy;

    // Core types for the unified API
    pub use crate::dispatch::Dispatch;
    pub use crate::handle::SyzygyHandle;
    pub use crate::model::Model;

    // CommandContext for command execution
    pub use crate::context::CommandContext;
    pub use crate::error::{ChannelClosed, ResourceNotFoundError, SyzygyError};

    // Handler types
    pub use crate::handler::{CommandHandler, EventHandler};

    // Magic handler types
    pub use crate::extract::{FromContainer, FromContainerMut};
    pub use crate::magic_handler::{MagicHandler, MagicHandlerExt, EventMagicHandler};

    // EventMap for zero-overhead dispatch
    pub use crate::event_map::{Event, EventMap, EventMapBuilder, EventVariant};

    // IndexedMap for generic zero-overhead storage
    pub use crate::indexed_map::{IndexedMap, Indexable, IndexArray};

    // ModelMap for multiple model support
    pub use crate::model_map::{ModelMap, ModelMapBuilder, ModelType};

    // Re-export the derive macros
    pub use syzygy_macros::{Event, ModelExtractors, ResourceExtractors};

    // Command executor for async execution
    #[cfg(feature = "async")]
    pub use crate::executor::{CommandExecutor, CommandExecutorHandle};

    // Deterministic testing utilities
    pub use crate::recorder::{EventRecorder, TimestampedEvent};
    pub use crate::replay::{EventReplayer, ReplayResult, TimingMode};
    pub use crate::test_utils::{TestUtils, TestScenario, TestConfig};
}
