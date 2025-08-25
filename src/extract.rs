//! Context-based extractors for magic handlers
//!
//! This module provides traits for extracting values from EventContext and EffectContext
//! to enable Axum-style magic parameter injection in handler functions.

use crate::async_context::EffectContext;
use crate::event_context::EventContext;
use crate::storage::{storage::Here, Selector};
use crate::error::ShellError;
use crossbeam_channel::Sender;

/// Extract a value from an EventContext
///
/// The Index parameter defaults to Here (head of storage chain) for convenience.
/// Most users won't need to specify it explicitly.
pub trait FromEventContext<'ctx, Event, Effect, Storage, Index = Here> {
    /// Extract Self from an EventContext
    fn from_context(ctx: &'ctx EventContext<Event, Effect, Storage>) -> Self;
}

/// Extract a value from an EffectContext  
///
/// The Index parameter defaults to Here (head of storage chain) for convenience.
/// Most users won't need to specify it explicitly.
pub trait FromEffectContext<'ctx, Event, Resources, Index = Here> {
    /// Extract Self from an EffectContext
    fn from_context(ctx: &'ctx EffectContext<Event, Resources>) -> Self;
}

// ============================================================================
// Common Extractor Implementations
// ============================================================================

/// Wrapper for extracting event sender from EffectContext
#[derive(Debug, Clone)]
pub struct EventSender<Event>(pub Sender<Event>);

impl<Event> EventSender<Event> {
    /// Send an event back to the Core
    pub fn send(&self, event: Event) -> Result<(), ShellError> {
        self.0.send(event).map_err(|_| ShellError::EventChannelClosed)
    }
}

// ============================================================================
// Core Extractor Implementations
// ============================================================================

// Extract EffectContext directly
impl<'ctx, Event, Resources> FromEffectContext<'ctx, Event, Resources> for EffectContext<Event, Resources>
where
    Event: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn from_context(ctx: &'ctx EffectContext<Event, Resources>) -> Self {
        ctx.clone()
    }
}

// Extract EventSender from EffectContext
impl<'ctx, Event, Resources> FromEffectContext<'ctx, Event, Resources> for EventSender<Event>
where
    Event: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn from_context(ctx: &'ctx EffectContext<Event, Resources>) -> Self {
        EventSender(ctx.event_sender().expect("EffectContext must have an event sender for EventSender extraction"))
    }
}

// Extract model references from EventContext
impl<'ctx, Event, Effect, Storage, T, I> FromEventContext<'ctx, Event, Effect, Storage, I> for &'ctx T
where
    Storage: Selector<T, I>,
{
    fn from_context(ctx: &'ctx EventContext<Event, Effect, Storage>) -> Self {
        ctx.model::<T, I>()
    }
}

// Extract mutable model references from EventContext  
impl<'ctx, Event, Effect, Storage, T, I> FromEventContext<'ctx, Event, Effect, Storage, I> for &'ctx mut T
where
    Storage: Selector<T, I>,
{
    fn from_context(ctx: &'ctx EventContext<Event, Effect, Storage>) -> Self {
        ctx.model_mut::<T, I>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::EmptyStorage;

    #[derive(Debug, Clone, Default)]
    struct TestModel {
        counter: i32,
        name: String,
    }

    #[derive(Debug, Clone)]
    struct TestResource {
        url: String,
    }

    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment,
    }

    #[derive(Debug, Clone)]
    enum TestEffect {
        Log(String),
    }

    #[derive(Debug, Clone, Default)]
    struct FirstModelType {
        value: i32,
    }

    #[test]
    fn test_model_ref_extraction() {
        let mut storage = EmptyStorage.with_model(TestModel {
            counter: 42,
            name: "test".to_string(),
        });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);
        let test_model: &TestModel = FromEventContext::from_context(&ctx);
        assert_eq!(test_model.counter, 42);
    }

    #[test]
    fn test_model_mut_extraction() {
        let mut storage = EmptyStorage.with_model(TestModel {
            counter: 123,
            name: "test".to_string(),
        });
        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let model_mut: &mut TestModel = FromEventContext::from_context(&ctx);
        assert_eq!(model_mut.counter, 123);
        model_mut.counter = 456;
        assert_eq!(model_mut.counter, 456);
    }

    #[test]
    fn test_event_two_model_ref_extraction() {
        let mut storage = EmptyStorage
            .with_model(TestModel {
                counter: 99,
                name: "tuple".to_string(),
            })
            .with_model(FirstModelType { value: 42 });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        // Extract two different model types from context
        let test_model: &TestModel = FromEventContext::from_context(&ctx);
        let first_model: &FirstModelType = FromEventContext::from_context(&ctx);

        assert_eq!(test_model.counter, 99);
        assert_eq!(first_model.value, 42);
    }

    #[test]
    fn test_event_two_model_mut_extraction() {
        let mut storage = EmptyStorage
            .with_model(TestModel {
                counter: 99,
                name: "tuple".to_string(),
            })
            .with_model(FirstModelType { value: 42 });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        // Extract two different model types mutably from context
        let test_model: &mut TestModel = FromEventContext::from_context(&ctx);
        let first_model: &mut FirstModelType = FromEventContext::from_context(&ctx);

        // Verify initial values
        assert_eq!(test_model.counter, 99);
        assert_eq!(first_model.value, 42);

        // Mutate both models
        test_model.counter = 200;
        test_model.name = "mutated".to_string();
        first_model.value = 84;

        // Verify mutations
        assert_eq!(test_model.counter, 200);
        assert_eq!(test_model.name, "mutated");
        assert_eq!(first_model.value, 84);
    }

    #[test]
    fn test_event_model_ref_and_model_mut_extraction() {
        let mut storage = EmptyStorage
            .with_model(TestModel {
                counter: 100,
                name: "original".to_string(),
            })
            .with_model(FirstModelType { value: 50 });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        // Extract one model as ref and another as mut
        let test_model_ref: &TestModel = FromEventContext::from_context(&ctx);
        let first_model_mut: &mut FirstModelType = FromEventContext::from_context(&ctx);

        // Verify initial values
        assert_eq!(test_model_ref.counter, 100);
        assert_eq!(test_model_ref.name, "original");
        assert_eq!(first_model_mut.value, 50);

        // Mutate only the mutable model
        first_model_mut.value = 150;

        // Verify the ref model is unchanged and mut model is changed
        assert_eq!(test_model_ref.counter, 100);
        assert_eq!(test_model_ref.name, "original");
        assert_eq!(first_model_mut.value, 150);
    }

    #[test]
    fn test_multi_model_extraction() {
        #[derive(Debug, Clone, Default)]
        struct SecondModel {
            value: i32,
        }

        let mut storage = EmptyStorage
            .with_model(TestModel { counter: 99, name: "test".to_string() })
            .with_model(SecondModel { value: 42 });

        let ctx = EventContext::<TestEvent, TestEffect, _>::new(&mut storage);

        let test_model: &TestModel = FromEventContext::from_context(&ctx);
        let second_model: &mut SecondModel = FromEventContext::from_context(&ctx);

        assert_eq!(test_model.counter, 99);
        assert_eq!(second_model.value, 42);

        second_model.value = 84;
        assert_eq!(second_model.value, 84);
    }

    #[test]
    fn test_effect_context_extraction() {
        let resources = EmptyStorage.with_model(TestResource {
            url: "test_url".to_string(),
        });
        let ctx = EffectContext::<TestEvent, _>::new(None, resources);

        let extracted_ctx: EffectContext<TestEvent, _> = FromEffectContext::from_context(&ctx);
        let resource: &TestResource = extracted_ctx.resource();
        assert_eq!(resource.url, "test_url");
    }

    #[test]
    fn test_event_sender_extraction() {
        use crossbeam_channel::unbounded;
        
        let (tx, rx) = unbounded();
        let resources = EmptyStorage;
        let ctx = EffectContext::<TestEvent, _>::new(Some(tx), resources);

        let sender: EventSender<TestEvent> = FromEffectContext::from_context(&ctx);
        sender.send(TestEvent::Increment).unwrap();
        assert!(rx.try_recv().is_ok());
    }

    #[test] 
    #[should_panic(expected = "EffectContext must have an event sender")]
    fn test_event_sender_extraction_panics_without_sender() {
        let resources = EmptyStorage;
        let ctx = EffectContext::<TestEvent, _>::new(None, resources);

        let _sender: EventSender<TestEvent> = FromEffectContext::from_context(&ctx);
    }
}

// ============================================================================
// Resource Extraction from EffectContext
// ============================================================================

// Extract resource references from EffectContext (zero-cost)
impl<'ctx, Event, Resources, T, I> FromEffectContext<'ctx, Event, Resources, I> for &'ctx T
where
    Resources: Selector<T, I>,
    Event: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn from_context(ctx: &'ctx EffectContext<Event, Resources>) -> Self {
        ctx.resource::<T, I>()
    }
}

