pub mod activity;
pub mod command;
pub mod core;
pub mod error;
pub mod extract;

#[cfg(feature = "shell")]
pub mod builder;
#[cfg(feature = "shell")]
pub mod executor;
#[cfg(feature = "shell")]
pub mod shell;
#[cfg(feature = "shell")]
pub mod syzygy;

pub mod prelude {
    pub use crate::command::builders as cmd;
    pub use crate::command::{Command, CommandStep};

    pub use crate::core::{Core, EventHandler, EventSender};

    pub use crate::extract::{EffectContext, EffectHandler, FromEffectContext};

    #[cfg(feature = "shell")]
    pub use crate::builder::SyzygyBuilder;
    #[cfg(feature = "shell")]
    pub use crate::executor::{ExecutorRegistry, InlineAsync, Plan, Task};
    #[cfg(feature = "shell")]
    pub use crate::shell::Shell;
    #[cfg(feature = "shell")]
    pub use crate::syzygy::{Runner, Syzygy, SyzygyConfig};

    pub use crate::error::{CommandError, CoreError, EffectError};
    #[cfg(feature = "shell")]
    pub use crate::error::ShellError;
}
