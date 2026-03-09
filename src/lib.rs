pub mod activity;
pub mod command;
pub mod core;
pub mod error;
pub mod extract;
pub mod macros;
pub mod resource;
pub mod test_store;

pub use syzygy_macros::Model;

#[cfg(feature = "shell")]
pub mod builder;
#[cfg(feature = "shell")]
pub mod executor;
#[cfg(feature = "shell")]
pub mod process;
#[cfg(feature = "shell")]
pub mod runner_tester;
#[cfg(feature = "shell")]
pub mod runtime;
#[cfg(feature = "shell")]
pub mod shell;
#[cfg(feature = "shell")]
pub mod subscription;
#[cfg(feature = "shell")]
pub mod syzygy;

pub mod prelude {
    pub use crate::command::builders as cmd;
    pub use crate::command::{AbortSlot, Command, CommandStep, TaskLease, TaskLeaseScope};

    pub use crate::core::{Core, EventSender};
    pub use crate::handle;
    pub use crate::resource::{Resource, ResourceMap};

    pub use crate::extract::{
        EffectContext, EffectHandler, EventContext, EventHandler, FromEffectContext, FutureEffect,
        Part, PartMut, StreamEffect,
    };
    pub use crate::test_store::{assert_panic, assert_panic_contains, Exhaustivity, TestStore};
    pub use syzygy_macros::Model;

    #[cfg(feature = "shell")]
    pub use crate::builder::SyzygyBuilder;
    #[cfg(feature = "shell")]
    pub use crate::executor::{BlockingCancelToken, Plan, Task};
    #[cfg(feature = "shell")]
    pub use crate::extract::{SubscriptionContext, SubscriptionHandler, SubscriptionPart};
    #[cfg(feature = "shell")]
    pub use crate::process::{
        CapturedOutput, ProcessError, ProcessErrorKind, ProcessExit, ProcessFrame, ProcessFraming,
        ProcessInput, ProcessOutput, ProcessSpec, ProcessTerminationPolicy, ProcessUpdate,
    };
    #[cfg(feature = "shell")]
    pub use crate::runner_tester::RunnerTester;
    #[cfg(feature = "shell")]
    pub use crate::shell::{
        Shell, ShellCancellationReason, ShellSnapshot, ShellSubscriptionSnapshot,
        ShellTaskSnapshot, ShellTerminationReason, ShellTerminationRecord, ShellTerminationTarget,
        ShellTraceCommandStep, ShellTraceConfig, ShellTraceEntry, ShellTraceEvent,
        ShellTraceProcessControl, ShellTraceTaskKind,
    };
    #[cfg(feature = "shell")]
    pub use crate::subscription::{
        Subscription, SubscriptionDriver, SubscriptionPollStart, SubscriptionStream,
    };
    #[cfg(feature = "shell")]
    pub use crate::syzygy::{
        DiagnosticsConfig, Runner, Syzygy, SyzygyConfig, UnhandledEffectPolicy,
    };

    #[cfg(feature = "shell")]
    pub use crate::error::ShellError;
    pub use crate::error::{CommandError, CoreError, EffectError};
}
