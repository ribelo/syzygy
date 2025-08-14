//! Event and command handlers for THE Syzygy system
//! 
//! Simple type aliases for event and command handlers.

use crate::context::CommandContext;
use crate::dispatch::Dispatch;
use crate::resource::Resources;
use std::future::Future;
use std::pin::Pin;

/// Event handler function type
/// 
/// Takes mutable model state, returns new events and commands.
pub type EventHandler<Event, Command, Model> = 
    fn(&mut Model) -> Dispatch<Event, Command>;

/// Command handler function type
/// 
/// Takes a CommandContext and command, returns a Future for async execution.
pub type CommandHandler<Event, Command> = 
    fn(CommandContext<Event, Resources>, Command) -> Pin<Box<dyn Future<Output = ()> + Send + 'static>>;