pub mod builder;
pub mod context;
pub mod dispatch;
pub mod error;
pub mod handle;
pub mod handler;
pub mod model;
pub mod resource;
pub mod syzygy;

#[cfg(feature = "async")]
pub mod executor;

pub mod prelude {
    // THE Syzygy - Simple enum-based system
    pub use crate::builder::SyzygyBuilder;
    pub use crate::syzygy::Syzygy;

    // Core types for the unified API
    pub use crate::dispatch::Dispatch;
    pub use crate::handle::SyzygyHandle;

    // CommandContext for command execution
    pub use crate::context::CommandContext;
    pub use crate::error::{ChannelClosed, ResourceNotFoundError, SyzygyError};

    // Handler types
    pub use crate::handler::{CommandHandler, EventHandler};

    // Command executor for async execution
    #[cfg(feature = "async")]
    pub use crate::executor::{CommandExecutor, CommandExecutorHandle};
}
