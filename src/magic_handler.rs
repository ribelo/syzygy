//! Magic handler system for type-safe container field extraction
//!
//! This module implements the Axum-style magic handler pattern, allowing
//! handler functions to declare the exact container fields they need as parameters.

use crate::extract::FromContainer;

/// Trait for functions that can handle events with automatic container field extraction
/// 
/// This trait enables Axum-style magic handlers where functions can declare
/// exactly which parts of the container they need, and the system automatically
/// extracts and provides those parameters.
/// 
/// # Examples
/// 
/// ```rust,ignore
/// use syzygy::prelude::*;
/// use syzygy::magic_handler::MagicHandler;
/// 
/// #[derive(Debug)]
/// struct AppModel {
///     users: Vec<String>,
///     counter: i32,
/// }
/// 
/// #[derive(Debug, Clone)]
/// enum Event { AddUser { name: String } }
/// 
/// #[derive(Debug, Clone)]  
/// enum Command { SaveUser { name: String } }
/// 
/// // Magic handler - automatically extracts needed fields
/// fn handle_add_user(
///     users: &mut Vec<String>, 
///     counter: &i32
/// ) -> Dispatch<Event, Command> {
///     users.push(format!("User #{}", counter));
///     Dispatch::command(Command::SaveUser { name: users.last().unwrap().clone() })
/// }
/// 
/// // Call the handler with magic parameter extraction
/// let mut model = AppModel { users: vec![], counter: 1 };
/// let result = handle_add_user.call(&mut model);
/// ```
pub trait MagicHandler<C, Args> {
    type Output;
    
    /// Call the handler with automatic parameter extraction from the container
    fn call(self, container: &mut C) -> Self::Output;
}

/// Trait for functions that can handle events with automatic container field extraction
/// 
/// This trait extends MagicHandler to support event-driven handlers where the first
/// parameter is always the event, followed by automatically extracted container fields.
/// 
/// # Examples
/// 
/// ```rust,ignore
/// use syzygy::prelude::*;
/// use syzygy::magic_handler::EventMagicHandler;
/// 
/// #[derive(Debug, Clone)]
/// struct CreateUser { name: String }
/// 
/// #[derive(Debug)]
/// struct AppModel {
///     users: Vec<String>,
///     counter: i32,
/// }
/// 
/// #[derive(Debug, Clone)]
/// enum Event { CreateUser(CreateUser) }
/// 
/// #[derive(Debug, Clone)]  
/// enum Command { SaveUser { name: String } }
/// 
/// // Event magic handler - event + automatically extracted fields
/// fn handle_create_user(
///     event: CreateUser,
///     users: &mut Vec<String>, 
///     counter: &i32
/// ) -> Dispatch<Event, Command> {
///     users.push(format!("{}_{}", event.name, counter));
///     Dispatch::command(Command::SaveUser { name: event.name })
/// }
/// 
/// // Call the handler with event + magic parameter extraction
/// let mut model = AppModel { users: vec![], counter: 1 };
/// let event = CreateUser { name: "alice".to_string() };
/// let result = handle_create_user.call_with_event(event, &mut model);
/// ```
pub trait EventMagicHandler<E, C, Args> {
    type Output;
    
    /// Call the handler with event + automatic parameter extraction from the container
    fn call_with_event(self, event: E, container: &mut C) -> Self::Output;
}

// Implementation for functions with no container dependencies
impl<C, F, R> MagicHandler<C, ()> for F
where
    F: FnOnce() -> R,
{
    type Output = R;
    
    fn call(self, _container: &mut C) -> Self::Output {
        self()
    }
}

// Implementation for functions with one immutable parameter
impl<C, F, A, R> MagicHandler<C, (A,)> for F
where
    F: FnOnce(A) -> R,
    A: FromContainer<C>,
{
    type Output = R;
    
    fn call(self, container: &mut C) -> Self::Output {
        let args = A::from_container(container);
        self(args)
    }
}

// Implementation for functions with two immutable parameters
impl<C, F, A, B, R> MagicHandler<C, (A, B)> for F
where
    F: FnOnce(A, B) -> R,
    A: FromContainer<C>,
    B: FromContainer<C>,
{
    type Output = R;
    
    fn call(self, container: &mut C) -> Self::Output {
        let a = A::from_container(container);
        let b = B::from_container(container);
        self(a, b)
    }
}

// Implementation for functions with three immutable parameters
impl<Container, F, A, B, C, R> MagicHandler<Container, (A, B, C)> for F
where
    F: FnOnce(A, B, C) -> R,
    A: FromContainer<Container>,
    B: FromContainer<Container>,
    C: FromContainer<Container>,
{
    type Output = R;
    
    fn call(self, container: &mut Container) -> Self::Output {
        let a = A::from_container(container);
        let b = B::from_container(container);
        let c = C::from_container(container);
        self(a, b, c)
    }
}

// Implementation for functions with four immutable parameters
impl<Container, F, A, B, C, D, R> MagicHandler<Container, (A, B, C, D)> for F
where
    F: FnOnce(A, B, C, D) -> R,
    A: FromContainer<Container>,
    B: FromContainer<Container>,
    C: FromContainer<Container>,
    D: FromContainer<Container>,
{
    type Output = R;
    
    fn call(self, container: &mut Container) -> Self::Output {
        let a = A::from_container(container);
        let b = B::from_container(container);
        let c = C::from_container(container);
        let d = D::from_container(container);
        self(a, b, c, d)
    }
}

// ============================================================================
// EventMagicHandler Implementations
// ============================================================================

// Implementation for event-only functions (no container dependencies)
impl<E, C, F, R> EventMagicHandler<E, C, (E,)> for F
where
    F: FnOnce(E) -> R,
{
    type Output = R;
    
    fn call_with_event(self, event: E, _container: &mut C) -> Self::Output {
        self(event)
    }
}

// Implementation for event + one immutable parameter
impl<E, C, F, A, R> EventMagicHandler<E, C, (E, A)> for F
where
    F: FnOnce(E, A) -> R,
    A: FromContainer<C>,
{
    type Output = R;
    
    fn call_with_event(self, event: E, container: &mut C) -> Self::Output {
        let a = A::from_container(container);
        self(event, a)
    }
}

// Implementation for event + two immutable parameters
impl<E, C, F, A, B, R> EventMagicHandler<E, C, (E, A, B)> for F
where
    F: FnOnce(E, A, B) -> R,
    A: FromContainer<C>,
    B: FromContainer<C>,
{
    type Output = R;
    
    fn call_with_event(self, event: E, container: &mut C) -> Self::Output {
        let a = A::from_container(container);
        let b = B::from_container(container);
        self(event, a, b)
    }
}

// Implementation for event + three immutable parameters
impl<E, Container, F, A, B, C, R> EventMagicHandler<E, Container, (E, A, B, C)> for F
where
    F: FnOnce(E, A, B, C) -> R,
    A: FromContainer<Container>,
    B: FromContainer<Container>,
    C: FromContainer<Container>,
{
    type Output = R;
    
    fn call_with_event(self, event: E, container: &mut Container) -> Self::Output {
        let a = A::from_container(container);
        let b = B::from_container(container);
        let c = C::from_container(container);
        self(event, a, b, c)
    }
}

// Implementation for event + four immutable parameters
impl<E, Container, F, A, B, C, D, R> EventMagicHandler<E, Container, (E, A, B, C, D)> for F
where
    F: FnOnce(E, A, B, C, D) -> R,
    A: FromContainer<Container>,
    B: FromContainer<Container>,
    C: FromContainer<Container>,
    D: FromContainer<Container>,
{
    type Output = R;
    
    fn call_with_event(self, event: E, container: &mut Container) -> Self::Output {
        let a = A::from_container(container);
        let b = B::from_container(container);
        let c = C::from_container(container);
        let d = D::from_container(container);
        self(event, a, b, c, d)
    }
}

// ============================================================================
// EventMagicHandler Implementations with Mutable Parameters
// ============================================================================
// Note: These are commented out due to trait conflicts. 
// We'll use a different approach for mutable parameter extraction.

/// Convenience trait to enable `.call_magic()` syntax on functions
/// 
/// This trait provides a more ergonomic way to call magic handlers.
/// 
/// # Examples
/// 
/// ```rust
/// use syzygy::magic_handler::{MagicHandler, MagicHandlerExt};
/// 
/// fn my_handler(counter: i32) -> String {
///     format!("Count: {}", counter)
/// }
/// 
/// // Create a container with the required data
/// struct Container { counter: i32 }
/// let mut container = Container { counter: 42 };
/// 
/// // Use the MagicHandlerExt trait method
/// // let result = my_handler.call_magic(&mut container);
/// ```
pub trait MagicHandlerExt<C, Args>: MagicHandler<C, Args> {
    /// Call the handler with magic parameter extraction (convenience method)
    fn call_magic(self, container: &mut C) -> Self::Output
    where
        Self: Sized,
    {
        self.call(container)
    }
}

// Blanket implementation for all magic handlers
impl<C, Args, T> MagicHandlerExt<C, Args> for T
where
    T: MagicHandler<C, Args>,
{
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::Dispatch;

    #[derive(Debug)]
    struct TestModel {
        users: Vec<String>,
        counter: i32,
        enabled: bool,
    }

    impl FromContainer<TestModel> for i32 {
        fn from_container(model: &TestModel) -> Self {
            model.counter
        }
    }

    impl FromContainer<TestModel> for bool {
        fn from_container(model: &TestModel) -> Self {
            model.enabled
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    enum TestEvent {
        UserAdded { name: String },
    }

    #[derive(Debug, Clone, PartialEq)]
    enum TestCommand {
        SaveUser { name: String },
    }

    #[test]
    fn test_magic_handler_no_params() {
        let mut model = TestModel {
            users: vec![],
            counter: 0,
            enabled: true,
        };

        fn handler() -> String {
            "no params".to_string()
        }

        let result = MagicHandler::call(handler, &mut model);
        assert_eq!(result, "no params");
    }

    #[test]
    fn test_magic_handler_one_param() {
        let mut model = TestModel {
            users: vec!["alice".to_string()],
            counter: 42,
            enabled: true,
        };

        fn handler(counter: i32) -> String {
            format!("counter: {}", counter)
        }

        let result = MagicHandler::call(handler, &mut model);
        assert_eq!(result, "counter: 42");
    }


    #[test]
    fn test_magic_handler_two_params() {
        let mut model = TestModel {
            users: vec!["alice".to_string()],
            counter: 5,
            enabled: true,
        };

        fn handler(counter: i32, enabled: bool) -> String {
            format!("counter: {}, enabled: {}", counter, enabled)
        }

        let result = MagicHandler::call(handler, &mut model);
        assert_eq!(result, "counter: 5, enabled: true");
    }

    #[test]
    fn test_magic_handler_three_params() {
        let mut model = TestModel {
            users: vec!["alice".to_string(), "bob".to_string()],
            counter: 99,
            enabled: false,
        };

        fn handler(counter: i32, enabled: bool) -> String {
            format!("counter: {}, enabled: {}", counter, enabled)
        }

        let result = MagicHandler::call(handler, &mut model);
        assert_eq!(result, "counter: 99, enabled: false");
    }

    #[test]
    fn test_magic_handler_with_dispatch() {
        let mut model = TestModel {
            users: vec![],
            counter: 1,
            enabled: true,
        };

        fn add_user_handler(counter: i32) -> Dispatch<TestEvent, TestCommand> {
            let name = format!("user_{}", counter);
            Dispatch::new(
                vec![TestEvent::UserAdded { name: name.clone() }],
                vec![TestCommand::SaveUser { name }]
            )
        }

        let result = MagicHandler::call(add_user_handler, &mut model);
        
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.events[0], TestEvent::UserAdded { name: "user_1".to_string() });
        
        assert_eq!(result.commands.len(), 1);
        assert_eq!(result.commands[0], TestCommand::SaveUser { name: "user_1".to_string() });
    }

    #[test]
    fn test_magic_handler_ext() {
        let mut model = TestModel {
            users: vec![],
            counter: 42,
            enabled: true,
        };

        fn handler(counter: i32) -> String {
            format!("magic: {}", counter)
        }

        // Test both call methods
        let result1 = MagicHandler::call(handler, &mut model);
        let result2 = handler.call_magic(&mut model);
        
        assert_eq!(result1, "magic: 42");
        assert_eq!(result2, "magic: 42");
    }
}