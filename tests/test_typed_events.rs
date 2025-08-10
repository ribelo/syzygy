//! Tests for the typed events system

use std::sync::atomic::{AtomicI32, AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use syzygy::prelude::*;

#[derive(Debug, Clone)]
struct TestModel {
    counter: i32,
    name: String,
}

impl Model for TestModel {
    type Snapshot = Self;
    fn to_snapshot(&self) -> Self::Snapshot {
        self.clone()
    }
}

// Test events
#[derive(Debug, Clone)]
struct IncrementEvent {
    amount: i32,
}

impl Event<TestModel> for IncrementEvent {
    fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
        syzygy.model.counter += self.amount;
    }
}

#[derive(Debug, Clone)]
struct SetNameEvent {
    name: String,
}

impl Event<TestModel> for SetNameEvent {
    fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
        syzygy.model.name = self.name;
    }
}

// Manual enum dispatch (since ambassador has issues)
#[derive(Debug, Clone)]
enum AppEvent {
    Increment(IncrementEvent),
    SetName(SetNameEvent),
}

impl Event<TestModel> for AppEvent {
    fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
        match self {
            AppEvent::Increment(event) => {
                syzygy.model.counter += event.amount;
            }
            AppEvent::SetName(event) => {
                syzygy.model.name = event.name;
            }
        }
    }
}

#[test]
fn test_enum_delegation() {
    let mut syzygy: Syzygy<TestModel, AppEvent> = Syzygy::builder()
        .model(TestModel {
            counter: 0,
            name: "initial".to_string(),
        })
        .build();

    // Dispatch typed events
    syzygy.dispatch(AppEvent::Increment(IncrementEvent { amount: 5 }));
    syzygy.dispatch(AppEvent::SetName(SetNameEvent { 
        name: "updated".to_string() 
    }));

    // Process events
    syzygy.handle_effects();

    // Verify state changes
    assert_eq!(syzygy.model.counter, 5);
    assert_eq!(syzygy.model.name, "updated");
}

#[test]
fn test_typed_events_single() {
    let mut syzygy: Syzygy<TestModel, IncrementEvent> = Syzygy::builder()
        .model(TestModel {
            counter: 0,
            name: "test".to_string(),
        })
        .build();

    // Dispatch typed event
    syzygy.dispatch(IncrementEvent { amount: 10 });
    syzygy.handle_effects();

    assert_eq!(syzygy.model.counter, 10);
}

#[test] 
fn test_closure_based_fallback() {
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel {
            counter: 0,
            name: "test".to_string(),
        })
        .build();

    // Unit events do nothing
    syzygy.dispatch(());

    syzygy.handle_effects();

    assert_eq!(syzygy.model.counter, 0);
    assert_eq!(syzygy.model.name, "test");
}

#[test]
fn test_mixed_events_and_closures() {
    let mut syzygy: Syzygy<TestModel, IncrementEvent> = Syzygy::builder()
        .model(TestModel {
            counter: 0,
            name: "test".to_string(),
        })
        .build();

    // Dispatch multiple typed events
    syzygy.dispatch(IncrementEvent { amount: 5 });
    syzygy.dispatch(IncrementEvent { amount: 5 });

    syzygy.handle_effects();

    assert_eq!(syzygy.model.counter, 10);
    assert_eq!(syzygy.model.name, "test");
}

#[tokio::test]
#[cfg(feature = "async")]
async fn test_tasks_with_typed_events() {
    let flag = Arc::new(AtomicBool::new(false));
    let counter_value = Arc::new(AtomicI32::new(0));
    
    let mut syzygy: Syzygy<TestModel, IncrementEvent> = Syzygy::builder()
        .model(TestModel {
            counter: 0,
            name: "test".to_string(),
        })
        .build();

    let flag_clone = flag.clone();
    let counter_clone = counter_value.clone();
    
    // Dispatch typed event
    syzygy.dispatch(IncrementEvent { amount: 42 });
    
    // Process the increment first
    syzygy.handle_effects();
    
    // Spawn task that reads snapshot after increment is processed
    syzygy.task(move |snapshot| async move {
        // Task sees snapshot of state at time of task spawn
        let model = snapshot.snapshot();
        counter_clone.store(model.counter, Ordering::SeqCst);
        
        // Simulate async work
        tokio::time::sleep(Duration::from_millis(10)).await;
        
        // Send event back to modify state (tasks can only dispatch events)
        // We'll reuse increment event for simplicity
        snapshot.dispatch(IncrementEvent { amount: 0 });
        
        flag_clone.store(true, Ordering::SeqCst);
    });
    
    // Process task spawn
    syzygy.handle_effects();
    
    // Give task time to run  
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_millis(50)).await;
        tokio::task::yield_now().await; // Force runtime to poll tasks
    }
    
    // Process any effects sent from task
    syzygy.handle_effects();
    
    // Debug output
    println!("Flag value: {}", flag.load(Ordering::SeqCst));
    println!("Counter value: {}", counter_value.load(Ordering::SeqCst));
    println!("Model counter: {}", syzygy.model.counter);
    println!("Model name: {}", syzygy.model.name);
    
    // Verify task ran
    assert!(flag.load(Ordering::SeqCst), "Task should have completed");
    assert_eq!(counter_value.load(Ordering::SeqCst), 42);
    assert_eq!(syzygy.model.counter, 42);
    assert_eq!(syzygy.model.name, "task_updated_42");
}

#[test]
fn test_event_ordering() {
    let mut syzygy: Syzygy<TestModel, IncrementEvent> = Syzygy::builder()
        .model(TestModel {
            counter: 0,
            name: "test".to_string(),
        })
        .build();

    // Dispatch multiple events
    syzygy.dispatch(IncrementEvent { amount: 1 });
    syzygy.dispatch(IncrementEvent { amount: 2 });
    syzygy.dispatch(IncrementEvent { amount: 3 });
    
    // Process all at once
    syzygy.handle_effects();

    // Should be processed in order: 0 + 1 + 2 + 3 = 6
    assert_eq!(syzygy.model.counter, 6);
}

#[test] 
fn test_unit_type_event() {
    let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
        .model(TestModel {
            counter: 0,
            name: "test".to_string(),
        })
        .build();

    // Dispatch unit event (should do nothing)
    syzygy.dispatch(());
    syzygy.handle_effects();

    // State unchanged
    assert_eq!(syzygy.model.counter, 0);
    assert_eq!(syzygy.model.name, "test");
}