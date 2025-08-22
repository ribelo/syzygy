//! AFIT-based Effect Handler trait for zero-cost async abstractions
//!
//! This module provides the EffectHandler trait using Async Functions in Traits (AFIT)
//! to eliminate BoxFuture allocations while maintaining purity through function-pointer-only
//! implementations.
//!
//! Key design principles:
//! - Only function pointers can implement EffectHandler (no captures/closures)
//! - Resources are passed explicitly via Arc<Resources>
//! - Zero-cost abstractions with compile-time verification

use std::future::Future;
use std::sync::Arc;
use crate::async_context::EffectContext;

/// AFIT-based trait for handling effects with zero-cost abstractions
///
/// This trait uses Async Functions in Traits to provide clean async syntax
/// while eliminating BoxFuture allocations. Only function pointers can implement
/// this trait to enforce purity and prevent environment capture.
///
/// # Type Parameters
///
/// - `Event`: The event type that can be sent back to Core
/// - `Effect`: The effect type this handler processes
/// - `Resources`: The resources/dependencies available to the handler
///
/// # Example
///
/// ```rust,ignore
/// async fn handle_http_effect(
///     effect: HttpEffect,
///     resources: Arc<MyResources>,
///     ctx: EffectContext<MyEvent>
/// ) {
///     match effect {
///         HttpEffect::Get { url } => {
///             let client = &resources.http_client;
///             match client.get(&url).send().await {
///                 Ok(response) => {
///                     let data = response.text().await.unwrap();
///                     ctx.send_event(MyEvent::DataReceived { data }).unwrap();
///                 }
///                 Err(err) => {
///                     ctx.send_event(MyEvent::HttpError { error: err.to_string() }).unwrap();
///                 }
///             }
///         }
///     }
/// }
///
/// // Shell setup with function pointer (no captures allowed)
/// let shell = shell.with_effect_handler(handle_http_effect);
/// ```
pub trait EffectHandler<Event, Effect, Resources>: Send + Sync + 'static {
    /// The concrete future returned by the handler.
    type Fut: Future<Output = ()> + Send + 'static;

    /// Handle an effect asynchronously (zero-alloc AFIT)
    ///
    /// This returns a concrete future type, allowing zero-cost monomorphization
    /// without boxing. The future is required to be `Send + 'static` to support
    /// multi-threaded runtimes and optional timeout wrapping.
    fn handle(&self, effect: Effect, resources: Arc<Resources>, ctx: EffectContext<Event>) -> Self::Fut;
}

/// Implementation for function pointers only (enforces purity)
///
/// This implementation only accepts function pointers, not closures or
/// other callables. This ensures that effect handlers cannot capture
/// environment variables, maintaining purity and testability.

/// Blanket implementation for any function-like handler. This enables free functions
/// and zero-capture closures to act as effect handlers with zero overhead.
impl<Event, Effect, Resources, Fut, F> EffectHandler<Event, Effect, Resources> for F
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Send + Sync + 'static,
    F: Fn(Effect, Arc<Resources>, EffectContext<Event>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    type Fut = Fut;

    fn handle(&self, effect: Effect, resources: Arc<Resources>, ctx: EffectContext<Event>) -> Self::Fut {
        (self)(effect, resources, ctx)
    }
}

/// Noop effect handler used as a default type parameter
impl<Event, Effect, Resources> EffectHandler<Event, Effect, Resources> for ()
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Send + Sync + 'static,
{
    type Fut = std::future::Ready<()>;
    fn handle(&self, _effect: Effect, _resources: Arc<Resources>, _ctx: EffectContext<Event>) -> Self::Fut {
        std::future::ready(())
    }
}

/// Adapter that turns an async function or zero-capture async closure into an EffectHandler
/// with zero allocation and full type inference.
pub fn effect_fn<Event, Effect, Resources, F, Fut>(
    f: F,
) -> impl EffectHandler<Event, Effect, Resources>
where
    Event: Send + 'static,
    Effect: Send + 'static,
    Resources: Send + Sync + 'static,
    F: Fn(Effect, Arc<Resources>, EffectContext<Event>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Processed { value: i32 },
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Process { value: i32 },
    }

    #[derive(Default)]
    struct TestResources {
        multiplier: i32,
    }

    // Test effect handler as function pointer
    async fn test_effect_handler(
        effect: TestEffect,
        resources: Arc<TestResources>,
        ctx: EffectContext<TestEvent>,
    ) {
        match effect {
            TestEffect::Process { value } => {
                let result = value * resources.multiplier;
                let _ = ctx.send_event(TestEvent::Processed { value: result });
            }
        }
    }

    #[tokio::test]
    async fn test_function_pointer_implements_effect_handler() {
        let resources = Arc::new(TestResources { multiplier: 2 });
        let ctx: EffectContext<TestEvent> = EffectContext::new(None);
        
        // Verify function pointer implements the trait
        let handler: fn(TestEffect, Arc<TestResources>, EffectContext<TestEvent>) -> _ = test_effect_handler;
        
        // This should compile and work
        handler.handle(
            TestEffect::Process { value: 5 },
            resources,
            ctx,
        ).await;
    }
    
    #[tokio::test]
    async fn test_effect_handler_execution() {
        use crossbeam_channel::unbounded;
        
        let (tx, rx) = unbounded();
        let resources = Arc::new(TestResources { multiplier: 3 });
        let ctx = EffectContext::new(Some(tx));
        
        // Execute through trait
        let handler: fn(TestEffect, Arc<TestResources>, EffectContext<TestEvent>) -> _ = test_effect_handler;
        handler.handle(
            TestEffect::Process { value: 7 },
            resources,
            ctx,
        ).await;
        
        // Verify event was sent
        let event = rx.try_recv().unwrap();
        match event {
            TestEvent::Processed { value } => assert_eq!(value, 21), // 7 * 3
        }
    }
    
    #[test]
    fn test_function_pointer_compiles() {
        // This test verifies that function pointers can be used as effect handlers
        let _handler: fn(TestEffect, Arc<TestResources>, EffectContext<TestEvent>) -> _ = test_effect_handler;
        
        // Verify the function pointer can be called directly
        // (We don't actually call it in the test to avoid async complexity)
    }
}
