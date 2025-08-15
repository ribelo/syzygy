//! Code generation for zero-overhead event dispatching
//!
//! This module contains functions that generate optimized match expressions
//! for event dispatching at compile time, eliminating runtime overhead.

use crate::builder::EventHandlerMetadata;

/// Generate a single handler match arm using crabtime
///
/// This generates a match arm that dispatches to a stored handler by index
#[crabtime::function]
pub fn generate_handler_match_arm(event_type: String, index: usize) -> String {
    crabtime::quote! {
        E::{{event_type}}(inner) => {
            // Call the stored handler at index {{index}}
            handlers[{{index}}](E::{{event_type}}(inner), model)
        },
    }
}

/// Generate a complete event dispatcher function using crabtime
///
/// This generates a dispatcher that uses an array of stored handlers
#[crabtime::function]
pub fn generate_runtime_dispatcher(match_arms: String) -> String {
    crabtime::quote! {
        |event: E, model: &mut M, handlers: &[Box<dyn Fn(E, &mut M) -> Dispatch<E, C> + Send + Sync + 'static>]| -> Dispatch<E, C> {
            match event {
{{match_arms}}                _ => Dispatch::none(),
            }
        }
    }
}

/// Generate match arms for event dispatching
///
/// This function generates individual match arms that directly call handler functions
/// without any runtime lookups or overhead.
#[must_use]
pub fn generate_match_arms(handlers: &[EventHandlerMetadata]) -> String {
    let mut arms = String::new();

    for handler in handlers {
        arms.push_str(&format!(
            "        E::{}(inner) => {}(inner, model),\n",
            handler.event_type_name,
            handler.handler_function_name
        ));
    }

    arms
}

/// Generate the complete event dispatcher function
///
/// This function generates a match expression that directly calls handler functions
/// without any runtime lookups or overhead. The generated code is identical to
/// hand-written match statements.
pub fn generate_event_dispatcher(handlers: &[EventHandlerMetadata]) -> String {
    let match_arms = generate_match_arms(handlers);
    
    format!(r#"/// Generated zero-overhead event dispatcher
///
/// This function is generated at compile time to provide direct dispatch
/// to registered event handlers without any runtime overhead.
pub fn generated_event_dispatcher<M, E, C>(
    event: E,
    model: &mut M
) -> crate::dispatch::Dispatch<E, C>
where
    E: Clone,
{{
    match event {{
{}        _ => crate::dispatch::Dispatch::none(),
    }}
}}"#, match_arms)
}

/// Generate individual handler function wrappers using string generation
///
/// This function generates wrapper functions for each event handler that
/// perform the magic parameter extraction and call the actual handler.
pub fn generate_handler_functions(handlers: &[EventHandlerMetadata]) -> String {
    let mut functions = String::new();

    for handler in handlers {
        let handler_fn = &handler.handler_function_name;
        let event_type = &handler.event_type_name;

        functions.push_str(&format!(r#"/// Generated handler function wrapper
pub fn {}<M, E, C>(
    event: {},
    model: &mut M
) -> crate::dispatch::Dispatch<E, C>
where
    E: Clone,
{{
    // Magic handler call with automatic field extraction
    // This will be expanded to call the actual stored handler function

    // For now, placeholder that returns empty dispatch
    // In full implementation, this would call the stored handler
    crate::dispatch::Dispatch::none()
}}

"#, handler_fn, event_type));
    }

    functions
}

/// Generate the complete event handling system using string generation
///
/// This combines both the dispatcher and handler functions into a complete
/// zero-overhead event handling system.
pub fn generate_complete_event_system(handlers: &[EventHandlerMetadata]) -> String {
    // Generate handler functions first
    let handler_functions = generate_handler_functions(handlers);

    // Then generate the main dispatcher
    let dispatcher = generate_event_dispatcher(handlers);

    format!(r#"/// Complete generated event handling system
///
/// This provides a zero-overhead event dispatching system with
/// automatic handler registration and type-safe dispatch.
pub mod generated_event_system {{
    use super::*;

{}

{}

    // Re-export the generated dispatcher
    pub use generated_event_dispatcher;
}}"#, handler_functions, dispatcher)
}

/// Generate the code as a formatted string for demonstration
/// This is a helper function that calls the string generation
pub fn generate_code_string(handlers: &[EventHandlerMetadata]) -> String {
    generate_complete_event_system(handlers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::TypeId;

    #[test]
    fn test_handler_metadata_creation() {
        let metadata = EventHandlerMetadata {
            event_type_name: "CreateUser".to_string(),
            handler_function_name: "handle_createuser".to_string(),
            type_id: TypeId::of::<String>(), // Using String as placeholder
            handler_tokens: Some("/* Handler for CreateUser */".to_string()),
        };

        assert_eq!(metadata.event_type_name, "CreateUser");
        assert_eq!(metadata.handler_function_name, "handle_createuser");
    }

    #[test]
    fn test_multiple_handlers() {
        let handlers = vec![
            EventHandlerMetadata {
                event_type_name: "CreateUser".to_string(),
                handler_function_name: "handle_createuser".to_string(),
                type_id: TypeId::of::<String>(),
                handler_tokens: Some("/* Handler for CreateUser */".to_string()),
            },
            EventHandlerMetadata {
                event_type_name: "UpdateUser".to_string(),
                handler_function_name: "handle_updateuser".to_string(),
                type_id: TypeId::of::<i32>(),
                handler_tokens: Some("/* Handler for UpdateUser */".to_string()),
            },
        ];

        assert_eq!(handlers.len(), 2);
        assert_eq!(handlers[0].event_type_name, "CreateUser");
        assert_eq!(handlers[1].event_type_name, "UpdateUser");
    }

    #[test]
    fn test_match_arms_generation() {
        let handlers = vec![
            EventHandlerMetadata {
                event_type_name: "CreateUser".to_string(),
                handler_function_name: "handle_createuser".to_string(),
                type_id: TypeId::of::<String>(),
                handler_tokens: Some("/* Handler for CreateUser */".to_string()),
            },
            EventHandlerMetadata {
                event_type_name: "UpdateUser".to_string(),
                handler_function_name: "handle_updateuser".to_string(),
                type_id: TypeId::of::<i32>(),
                handler_tokens: Some("/* Handler for UpdateUser */".to_string()),
            },
        ];

        let match_arms = generate_match_arms(&handlers);
        assert!(match_arms.contains("E::CreateUser(inner) => handle_createuser(inner, model)"));
        assert!(match_arms.contains("E::UpdateUser(inner) => handle_updateuser(inner, model)"));
    }

    #[test]
    fn test_complete_code_generation() {
        let handlers = vec![
            EventHandlerMetadata {
                event_type_name: "CreateUser".to_string(),
                handler_function_name: "handle_createuser".to_string(),
                type_id: TypeId::of::<String>(),
                handler_tokens: Some("/* Handler for CreateUser */".to_string()),
            },
        ];

        let generated_code = generate_code_string(&handlers);
        assert!(generated_code.contains("pub mod generated_event_system"));
        assert!(generated_code.contains("pub fn generated_event_dispatcher"));
        assert!(generated_code.contains("pub fn handle_createuser"));
    }
}
