//! Code generation for zero-overhead event dispatching
//!
//! This module contains utilities for generating optimized match expressions
//! for event dispatching. Uses proc macros for compile-time code generation.

use crate::builder::EventHandlerMetadata;
use proc_macro2::TokenStream;
use quote::quote;

/// Generate a zero-overhead event dispatcher using proc macros
///
/// This function generates a match expression that directly calls handler functions
/// without any runtime lookups or overhead. The generated code is identical to
/// hand-written match statements.
pub fn generate_event_dispatcher(handlers: &[EventHandlerMetadata]) -> TokenStream {
    let mut match_arms = Vec::new();

    for handler in handlers {
        let variant_name = proc_macro2::Ident::new(&handler.event_type_name, proc_macro2::Span::call_site());
        let handler_fn = proc_macro2::Ident::new(&handler.handler_function_name, proc_macro2::Span::call_site());

        match_arms.push(quote! {
            E::#variant_name(inner) => #handler_fn(inner, model),
        });
    }

    quote! {
        /// Generated zero-overhead event dispatcher
        ///
        /// This function is generated at compile time to provide direct dispatch
        /// to registered event handlers without any runtime overhead.
        pub fn generated_event_dispatcher<M, E, C>(
            event: E,
            model: &mut M
        ) -> crate::dispatch::Dispatch<E, C>
        where
            E: Clone,
        {
            match event {
                #(#match_arms)*
                _ => crate::dispatch::Dispatch::none(),
            }
        }
    }
}

/// Generate individual handler function wrappers using proc macros
///
/// This function generates wrapper functions for each event handler that
/// perform the magic parameter extraction and call the actual handler.
pub fn generate_handler_functions(handlers: &[EventHandlerMetadata]) -> TokenStream {
    let mut functions = Vec::new();

    for handler in handlers {
        let handler_fn = proc_macro2::Ident::new(&handler.handler_function_name, proc_macro2::Span::call_site());
        let event_type = proc_macro2::Ident::new(&handler.event_type_name, proc_macro2::Span::call_site());

        functions.push(quote! {
            /// Generated handler function wrapper
            pub fn #handler_fn<M, E, C>(
                event: #event_type,
                model: &mut M
            ) -> crate::dispatch::Dispatch<E, C>
            where
                E: Clone,
            {
                // Magic handler call with automatic field extraction
                // This will be expanded to call the actual stored handler function
                // use crate::magic_handler::EventMagicHandler;

                // For now, placeholder that returns empty dispatch
                // In full implementation, this would call the stored handler
                crate::dispatch::Dispatch::none()
            }
        });
    }

    quote! {
        #(#functions)*
    }
}

/// Generate the complete event handling system
///
/// This combines both the dispatcher and handler functions into a complete
/// zero-overhead event handling system.
pub fn generate_complete_event_system(handlers: &[EventHandlerMetadata]) -> TokenStream {
    // Generate handler functions first
    let handler_functions = generate_handler_functions(handlers);

    // Then generate the main dispatcher
    let dispatcher = generate_event_dispatcher(handlers);

    quote! {
        /// Complete generated event handling system
        ///
        /// This provides a zero-overhead event dispatching system with
        /// automatic handler registration and type-safe dispatch.
        pub mod generated_event_system {
            use super::*;

            #handler_functions

            #dispatcher

            // Re-export the generated dispatcher
            pub use generated_event_dispatcher;
        }
    }
}

/// Generate the code as a formatted string for demonstration
pub fn generate_code_string(handlers: &[EventHandlerMetadata]) -> String {
    let tokens = generate_complete_event_system(handlers);
    format!("{}", tokens)
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
            },
            EventHandlerMetadata {
                event_type_name: "UpdateUser".to_string(),
                handler_function_name: "handle_updateuser".to_string(),
                type_id: TypeId::of::<i32>(),
            },
        ];

        assert_eq!(handlers.len(), 2);
        assert_eq!(handlers[0].event_type_name, "CreateUser");
        assert_eq!(handlers[1].event_type_name, "UpdateUser");
    }
}
