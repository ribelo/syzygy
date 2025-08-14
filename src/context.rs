//! Command execution context for THE Syzygy system
//!
//! Provides CommandContext for commands to interact with the system through
//! resource access and event dispatch capabilities.

use crossbeam_channel::Sender;
use crate::error::SyzygyError;
use crate::resource::{ResourceAccess, Resources};

/// Command context providing capabilities for commands to interact with the system
///
/// `CommandContext` provides commands with capabilities:
/// - Event dispatch capability
/// - Resource access capability  
/// - Effect tracking/tracing capability
///
/// Resources are accessible through compile-time chains for zero-overhead access.
///
/// # Examples
///
/// ```rust,ignore
/// use syzygy::prelude::*;
///
/// #[derive(Debug, Clone)]
/// enum AppCommand {
///     SaveUser { user: User },
///     FetchPosts { user_id: u32, limit: usize },
/// }
///
/// impl<E, R> Command<E, R> for AppCommand
/// where
///     E: Clone + Send,
///     R: Send,
/// {
///     async fn execute(&self, ctx: CommandContext<E, R>) {
///         match self {
///             AppCommand::SaveUser { user } => {
///                 let db: Arc<Database> = ctx.resource();
///                 // if db.save(user).await.is_ok() {
///                 //     let _ = ctx.dispatch(AppEvent::SaveSuccess);
///                 // } else {
///                 //     let _ = ctx.dispatch(AppEvent::SaveFailed);
///                 // }
///             }
///             AppCommand::FetchPosts { user_id, limit } => {
///                 let api: Arc<ApiClient> = ctx.resource();
///                 // match api.fetch_posts(*user_id, *limit).await {
///                 //     Ok(posts) => {
///                 //         let _ = ctx.dispatch(AppEvent::PostsFetched { posts });
///                 //     }
///                 //     Err(e) => {
///                 //         let _ = ctx.dispatch(AppEvent::FetchError { error: e });
///                 //     }
///                 // }
///             }
///         }
///     }
/// }
/// ```
pub struct CommandContext<Event, Resources> 
where
    Event: Clone,
{
    event_tx: Sender<Event>,
    resources: Resources,
}

impl<Event, Resources> CommandContext<Event, Resources> 
where
    Event: Clone,
{
    /// Create a new CommandContext
    pub(crate) fn new(event_tx: Sender<Event>, resources: Resources) -> Self {
        Self {
            event_tx,
            resources,
        }
    }
    
    /// Dispatch an event back to the system
    /// 
    /// This allows commands to trigger new events as a result of their execution.
    /// 
    /// # Errors
    /// 
    /// Returns `SyzygyError::ChannelClosed` if the event channel is closed.
    /// This typically happens during system shutdown.
    /// 
    /// # Examples
    /// 
    /// ```rust,ignore
    /// # use syzygy::prelude::*;
    /// # use syzygy::error::SyzygyError;
    /// async fn save_user<E, R>(ctx: &CommandContext<E, R>, user: User) -> Result<(), SyzygyError>
/// where
///     E: Clone,
/// {
    ///     let db: Arc<Database> = ctx.resource();
    ///     
    ///     match db.save(&user).await {
    ///         Ok(_) => (), // ctx.dispatch(AppEvent::UserSaved { id: user.id }),
    ///         Err(e) => (), // ctx.dispatch(AppEvent::SaveFailed { error: e.to_string() }),
    ///     }
    ///     Ok(())
    /// }
    /// ```
    pub fn dispatch(&self, event: Event) -> Result<(), SyzygyError> {
        self.event_tx.send(event)
            .map_err(|_| SyzygyError::ChannelClosed)
    }
}

// Implement ResourceAccess for CommandContext
impl<Event> ResourceAccess for CommandContext<Event, Resources> 
where
    Event: Clone,
{
    fn resources(&self) -> &Resources {
        &self.resources
    }
}

impl<Event, Resources> std::fmt::Debug for CommandContext<Event, Resources> 
where
    Event: Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CommandContext")
            .field("event_tx", &"Sender<Event>")
            .field("resources", &std::any::type_name::<Resources>())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use crate::resource::Resources;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    enum TestEvent {
        TestMsg,
    }

    #[test]
    fn test_command_context() {
        let (event_tx, _event_rx) = unbounded();
        let resources = Resources::default();
        let ctx = CommandContext::new(event_tx, resources);
        
        // Test dispatch
        assert!(ctx.dispatch(TestEvent::TestMsg).is_ok());
        
        // Test debug formatting
        let debug_str = format!("{:?}", ctx);
        assert!(debug_str.contains("CommandContext"));
    }
    
    #[test]
    fn test_command_context_with_resources() {
        #[derive(Debug)]
        struct Database {
            connected: bool,
        }
        
        let (event_tx, _event_rx) = unbounded::<String>();
        let mut resources = Resources::default();
        resources.insert(Database { connected: true });
        let ctx = CommandContext::new(event_tx, resources);
        
        // Test resource access
        let db: Arc<Database> = ctx.resource();
        assert!(db.connected);
    }
}