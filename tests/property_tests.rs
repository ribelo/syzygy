//! Property-based tests for Syzygy Commands and App trait invariants
//!
//! This module implements property-based testing to verify fundamental invariants:
//! 1. update() function purity (same input = same output)
//! 2. No panics on any input combination  
//! 3. Model state consistency after updates
//! 4. Command composition properties

use proptest::prelude::*;
use syzygy::prelude::*;
use syzygy::event_context::EventContext;
use std::panic;

#[derive(Debug, Clone, PartialEq)]
enum TestEvent {
    Increment,
    Decrement,
    SetValue(i32),
    Reset,
    Multiply(i32),
    Error(String),
}

#[derive(Debug, Clone, PartialEq)]
enum TestEffect {
    Log(String),
    Save(i32),
    Validate(i32),
}

#[derive(Debug, Default, Clone, PartialEq)]
struct TestModel {
    counter: i32,
    max_value: i32,
    history: Vec<i32>,
}

use syzygy::storage::{Storage, EmptyStorage};

fn test_update(
    event: TestEvent,
    ctx: &mut EventContext<TestEvent, TestEffect, Storage<TestModel, EmptyStorage>>,
) -> Command<TestEvent, TestEffect> {
    let model: &mut TestModel = ctx.model_mut();
        match event {
            TestEvent::Increment => {
                if model.counter < model.max_value {
                    model.counter += 1;
                    model.history.push(model.counter);
                    Command::effect(TestEffect::Log(format!("Incremented to {}", model.counter)))
                } else {
                    Command::event(TestEvent::Error("Max value reached".to_string()))
                }
            }
            TestEvent::Decrement => {
                model.counter = (model.counter - 1).max(0); // Ensure never negative
                model.history.push(model.counter);
                Command::effect(TestEffect::Log(format!("Decremented to {}", model.counter)))
            }
            TestEvent::SetValue(val) => {
                let old_value = model.counter;
                model.counter = val.clamp(0, model.max_value);
                model.history.push(model.counter);
                
                if old_value == model.counter {
                    Command::none()
                } else {
                    Command::effect(TestEffect::Save(model.counter))
                }
            }
            TestEvent::Reset => {
                model.counter = 0;
                model.history.clear();
                Command::batch([
                    Command::effect(TestEffect::Log("Reset".to_string())),
                    Command::effect(TestEffect::Save(0)),
                ])
            }
            TestEvent::Multiply(factor) => {
                if factor == 0 {
                    Command::event(TestEvent::Error("Cannot multiply by zero".to_string()))
                } else if factor < 0 {
                    Command::event(TestEvent::Error("Cannot multiply by negative".to_string()))
                } else {
                    let new_val = model.counter.saturating_mul(factor);
                    model.counter = new_val.min(model.max_value);
                    model.history.push(model.counter);
                    Command::effect(TestEffect::Validate(model.counter))
                }
            }
            TestEvent::Error(msg) => {
                // Error events should be handled gracefully without panicking
                Command::effect(TestEffect::Log(format!("Error: {msg}")))
            }
        }
}

// Strategy for generating arbitrary TestEvents
fn test_event_strategy() -> impl Strategy<Value = TestEvent> {
    prop_oneof![
        Just(TestEvent::Increment),
        Just(TestEvent::Decrement),
        any::<i32>().prop_map(TestEvent::SetValue),
        Just(TestEvent::Reset),
        any::<i32>().prop_map(TestEvent::Multiply),
        ".*".prop_map(TestEvent::Error),
    ]
}

// Strategy for generating arbitrary TestModel instances
fn test_model_strategy() -> impl Strategy<Value = TestModel> {
    (0i32..1000i32, 0i32..1000i32, proptest::collection::vec(0i32..1000i32, 0..10))
        .prop_map(|(counter, max_value, history)| TestModel {
            counter: counter.min(max_value), // Ensure counter <= max_value
            max_value,
            history,
        })
}

#[cfg(test)]
mod property_tests {
    use super::*;

    proptest! {
        /// Property: update() function is pure - same inputs produce same outputs
        #[test]
        fn update_function_is_pure(
            event in test_event_strategy(),
            model in test_model_strategy(),
        ) {
            
            // Call update twice with identical inputs
            let model1 = model.clone();
            let model2 = model.clone();
            
            let mut storage1 = EmptyStorage.with_model(model1);
            let mut storage2 = EmptyStorage.with_model(model2);
            let command1 = {
                let mut ctx = EventContext::new(&mut storage1);
                test_update(event.clone(), &mut ctx)
            };
            let command2 = {
                let mut ctx = EventContext::new(&mut storage2);
                test_update(event, &mut ctx)
            };
            
            // Get models back from storage
            let model1: &TestModel = storage1.get();
            let model2: &TestModel = storage2.get();
            
            // Models should be identical after identical operations
            prop_assert_eq!(model1, model2);
            
            // Commands should be equivalent (same effects/events)
            prop_assert_eq!(command1.len(), command2.len());
            
            // Convert to vectors for comparison since Commands don't impl PartialEq
            let outputs1: Vec<_> = command1.into_iter().collect();
            let outputs2: Vec<_> = command2.into_iter().collect();
            prop_assert_eq!(outputs1, outputs2);
        }
        
        /// Property: update() never panics on any input combination
        #[test]
        fn update_never_panics(
            event in test_event_strategy(),
            model in test_model_strategy(),
        ) {
            let mut test_model = model;
            
            // Catch any panics
            let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                { 
                    let mut storage = EmptyStorage.with_model(test_model);
                    {
            let mut ctx = EventContext::new(&mut storage);
            test_update(event, &mut ctx)
        };
                    test_model = storage.get().clone();
                }
            }));
            
            prop_assert!(result.is_ok(), "update() panicked on valid input");
        }
        
        /// Property: Model state remains consistent after updates
        #[test]
        fn model_state_consistency(
            event in test_event_strategy(),
            model in test_model_strategy(),
        ) {
            let mut test_model = model.clone();
            let original_max = test_model.max_value;
            
            let mut storage = EmptyStorage.with_model(test_model);
            let _command = {
            let mut ctx = EventContext::new(&mut storage);
            test_update(event, &mut ctx)
        };
            test_model = storage.get().clone();
            
            // Invariants that should always hold:
            
            // 1. Counter should never exceed max_value
            prop_assert!(test_model.counter <= test_model.max_value, 
                "Counter {} exceeded max_value {}", test_model.counter, test_model.max_value);
            
            // 2. Counter should never be negative (our app enforces this)
            prop_assert!(test_model.counter >= 0, 
                "Counter became negative: {}", test_model.counter);
            
            // 3. max_value should never change (our app doesn't modify it)
            prop_assert_eq!(test_model.max_value, original_max, 
                "max_value was unexpectedly modified");
            
            // 4. History should grow or reset, never shrink partially
            // (Reset clears it, other operations grow it)
            prop_assert!(test_model.history.len() <= model.history.len() + 1 || test_model.history.is_empty(),
                "History length changed unexpectedly");
        }
        
        /// Property: Command composition preserves structure
        #[test]
        fn command_composition_properties(
            events in proptest::collection::vec(test_event_strategy(), 1..5),
        ) {
            let model = TestModel { counter: 0, max_value: 100, history: vec![] };
            
            // Generate commands from events
            let commands: Vec<_> = events.iter().map(|event| {
                { 
                    let mut storage = EmptyStorage.with_model(model.clone());
                    {
            let mut ctx = EventContext::new(&mut storage);
            test_update(event.clone(), &mut ctx))
                }
            }).collect();
            
            // Test batch composition
            let batched = Command::batch(commands.clone());
            let individual_total: usize = commands.iter().map(syzygy::command::Command::len).sum();
            
            prop_assert_eq!(batched.len(), individual_total, 
                "Batch composition should preserve total output count");
            
            // Test monadic composition maintains structure
            let mut chained = Command::none();
            for cmd in commands {
                chained = chained.then(cmd);
            }
            
            prop_assert_eq!(chained.len(), individual_total,
                "Monadic composition should preserve total output count");
        }
        
        /// Property: Command filtering preserves type safety
        #[test]
        fn command_filtering_properties(
            event in test_event_strategy(),
            model in test_model_strategy(),
        ) {
            let mut test_model = model;
            
            let mut storage = EmptyStorage.with_model(test_model);
            let command = {
            let mut ctx = EventContext::new(&mut storage);
            test_update(event, &mut ctx)
        };
            test_model = storage.get().clone();
            let original_len = command.len();
            
            // Filter out effects
            let events_only = command.clone().filter(|output| {
                matches!(output, CommandStep::Event(_))
            });
            
            // Filter out events  
            let effects_only = command.filter(|output| {
                matches!(output, CommandStep::Effect(_))
            });
            
            // Combined length should not exceed original
            prop_assert!(events_only.len() + effects_only.len() <= original_len,
                "Filtered commands exceed original length");
            
            // All outputs should be valid
            for output in events_only {
                prop_assert!(matches!(output, CommandStep::Event(_)),
                    "Events-only filter contained non-event");
            }
            
            for output in effects_only {
                prop_assert!(matches!(output, CommandStep::Effect(_)),
                    "Effects-only filter contained non-effect");
            }
        }
        
        /// Property: Event sequences maintain model invariants
        #[test]
        fn event_sequence_invariants(
            events in proptest::collection::vec(test_event_strategy(), 0..20),
            initial_model in test_model_strategy(),
        ) {
            let mut model = initial_model.clone();
            let original_max = model.max_value;
            
            // Process sequence of events
            for event in events {
                let mut storage = EmptyStorage.with_model(model);
                let _command = {
            let mut ctx = EventContext::new(&mut storage);
            test_update(event, &mut ctx)
        };
                model = storage.get().clone();
                
                // Check invariants after each event
                prop_assert!(model.counter >= 0, "Counter became negative");
                prop_assert!(model.counter <= model.max_value, "Counter exceeded max");
                prop_assert_eq!(model.max_value, original_max, "max_value changed");
            }
        }
    }
}

#[cfg(test)]
mod command_monadic_properties {
    use super::*;

    proptest! {
        /// Property: and_then composition is associative when transformations commute
        #[test]
        fn and_then_associativity(
            event in test_event_strategy(),
            model in test_model_strategy(),
        ) {
            let mut test_model = model;
            
            let mut storage = EmptyStorage.with_model(test_model);
            let base_command = {
            let mut ctx = EventContext::new(&mut storage);
            test_update(event, &mut ctx)
        };
            test_model = storage.get().clone();
            
            // Define transformations
            let f = |_outputs: &[CommandStep<TestEvent, TestEffect>]| {
                Command::effect(TestEffect::Log("f".to_string()))
            };
            let g = |_outputs: &[CommandStep<TestEvent, TestEffect>]| {
                Command::effect(TestEffect::Log("g".to_string()))
            };
            
            // Test associativity: (cmd.and_then(f)).and_then(g) == cmd.and_then(|o| f(o).and_then(g))
            let left = base_command.clone().and_then(f).and_then(g);
            let right = base_command.and_then(|outputs| f(outputs).and_then(g));
            
            prop_assert_eq!(left.len(), right.len(),
                "and_then associativity violated");
        }
        
        /// Property: when(true) is identity, when(false) produces empty command
        #[test]
        fn when_identity_and_empty(
            event in test_event_strategy(),
            model in test_model_strategy(),
        ) {
            let mut test_model = model;
            
            let mut storage = EmptyStorage.with_model(test_model);
            let command = {
            let mut ctx = EventContext::new(&mut storage);
            test_update(event, &mut ctx)
        };
            test_model = storage.get().clone();
            let original_len = command.len();
            
            // when(true) should be identity
            let when_true = command.clone().when(true);
            prop_assert_eq!(when_true.len(), original_len,
                "when(true) should preserve command");
            
            // when(false) should produce empty command
            let when_false = command.when(false);
            prop_assert_eq!(when_false.len(), 0,
                "when(false) should produce empty command");
        }
        
        /// Property: or_else with non-empty left side ignores right side
        #[test]
        fn or_else_left_precedence(
            event in test_event_strategy(),
            model in test_model_strategy(),
        ) {
            let mut test_model = model;
            
            let mut storage = EmptyStorage.with_model(test_model);
            let command = {
            let mut ctx = EventContext::new(&mut storage);
            test_update(event, &mut ctx)
        };
            test_model = storage.get().clone();
            
            if !command.is_empty() {
                let fallback = Command::effect(TestEffect::Log("fallback".to_string()));
                let result = command.clone().or_else(fallback);
                
                prop_assert_eq!(result.len(), command.len(),
                    "or_else should ignore fallback when left side is non-empty");
            }
        }
        
        /// Property: then is cumulative
        #[test]
        fn then_cumulative(
            event in test_event_strategy(),
            model in test_model_strategy(),
        ) {
            let mut test_model = model;
            
            let mut storage = EmptyStorage.with_model(test_model);
            let cmd1 = {
            let mut ctx = EventContext::new(&mut storage);
            test_update(event, &mut ctx)
        };
            test_model = storage.get().clone();
            let cmd2 = Command::effect(TestEffect::Log("additional".to_string()));
            
            let combined = cmd1.clone().then(cmd2.clone());
            
            prop_assert_eq!(combined.len(), cmd1.len() + cmd2.len(),
                "then should combine command lengths");
        }
    }
}
