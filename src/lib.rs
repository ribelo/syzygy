pub mod activity;
pub mod command;
pub mod core;
pub mod dependency;
pub mod error;
pub mod extract;
pub mod feature;
pub mod if_let;
pub mod macros;
pub mod reducer;
pub mod test_store;

pub use syzygy_macros::Model;

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
    pub use crate::dependency::{Resource, ResourceMap};
    pub use crate::dispatch;

    pub use crate::extract::{
        EffectContext, EffectHandler, EventContext, EventHandler, ExtractMutFrom,
        FromEffectContext, FromEventContext, FutureEffect, StreamEffect,
    };
    pub use crate::feature::Feature;
    pub use crate::if_let::{if_let, IfLet};
    pub use crate::reducer::{
        combine, BoxedReducer, Combine, Combined, DebugReducer, ForEach, OnChange, Reduce, Reducer,
        ReducerExt, Scope,
    };
    pub use crate::test_store::{assert_panic, assert_panic_contains, Exhaustivity, TestStore};
    pub use syzygy_macros::Model;

    #[cfg(feature = "shell")]
    pub use crate::builder::SyzygyBuilder;
    #[cfg(feature = "shell")]
    pub use crate::executor::{Plan, Task};
    #[cfg(feature = "shell")]
    pub use crate::shell::Shell;
    #[cfg(feature = "shell")]
    pub use crate::syzygy::{Runner, Syzygy, SyzygyConfig};

    #[cfg(feature = "shell")]
    pub use crate::error::ShellError;
    pub use crate::error::{CommandError, CoreError, EffectError};
}
