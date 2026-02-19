pub mod activity;
pub mod command;
pub mod core;
pub mod error;
pub mod extract;
pub mod reducer;
pub mod test_store;

pub use syzygy_macros::{Model, Resources};

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
    pub use crate::command::{CancelId, Command, CommandStep};

    pub use crate::core::{Core, EventSender};

    pub use crate::extract::{
        EffectContext, EffectHandler, EventContext, EventHandler, FromEffectContext,
        FromEventContext,
    };
    pub use crate::reducer::{combine, BoxedReducer, Combine, Reduce, Reducer, ReducerExt, Scope};
    pub use crate::test_store::{assert_panic, assert_panic_contains, TestStore};
    pub use syzygy_macros::{Model, Resources};

    #[cfg(feature = "shell")]
    pub use crate::builder::SyzygyBuilder;
    #[cfg(feature = "shell")]
    pub use crate::executor::{ExecutorRegistry, InlineAsync, Plan, Task};
    #[cfg(feature = "shell")]
    pub use crate::shell::Shell;
    #[cfg(feature = "shell")]
    pub use crate::syzygy::{Runner, Syzygy, SyzygyConfig};

    #[cfg(feature = "shell")]
    pub use crate::error::ShellError;
    pub use crate::error::{CommandError, CoreError, EffectError};
}
