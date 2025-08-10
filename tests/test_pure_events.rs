// Test the pure event system

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

#[derive(Debug)]
struct IncrementEvent {
    amount: i32,
}

impl Event<TestModel> for IncrementEvent {
    fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
        syzygy.model_mut().counter += self.amount;
    }
}

#[derive(Debug)]
struct SetNameEvent {
    name: String,
}

impl Event<TestModel> for SetNameEvent {
    fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
        syzygy.model_mut().name = self.name;
    }
}

#[derive(Debug)]
struct ChainEvent;

impl Event<TestModel> for ChainEvent {
    fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
        syzygy.model_mut().counter += 1;
        if syzygy.model().counter < 5 {
            syzygy.dispatch(ChainEvent);
        }
    }
}

#[test]
fn test_simple_event() {
    let mut syzygy: Syzygy<TestModel, IncrementEvent> = Syzygy::builder()
        .model(TestModel { counter: 0, name: "test".to_string() })
        .build();
    
    // Dispatch event
    syzygy.dispatch(IncrementEvent { amount: 5 });
    
    // Event should be queued but not processed yet
    assert_eq!(syzygy.model().counter, 0);
    
    // Process effects
    syzygy.handle_effects();
    
    // Event should be processed now
    assert_eq!(syzygy.model().counter, 5);
}

#[test]
fn test_multiple_events() {
    let mut syzygy: Syzygy<TestModel, IncrementEvent> = Syzygy::builder()
        .model(TestModel { counter: 0, name: "test".to_string() })
        .build();
    
    // Dispatch multiple events
    syzygy.dispatch(IncrementEvent { amount: 1 });
    syzygy.dispatch(IncrementEvent { amount: 2 });
    syzygy.dispatch(IncrementEvent { amount: 3 });
    
    // Events are queued but not processed yet
    
    // Process all events
    syzygy.handle_effects();
    
    assert_eq!(syzygy.model().counter, 6); // 1 + 2 + 3
}

#[test]
fn test_processing() {
    let mut syzygy: Syzygy<TestModel, IncrementEvent> = Syzygy::builder()
        .model(TestModel { counter: 0, name: "test".to_string() })
        .build();
    
    // Dispatch many events
    for i in 1..=10 {
        syzygy.dispatch(IncrementEvent { amount: i });
    }
    
    // Events are queued but not processed yet
    
    // Process all events
    syzygy.handle_effects();
    assert_eq!(syzygy.model().counter, 55); // sum of 1..=10
}

#[test]
fn test_chaining_events() {
    let mut syzygy: Syzygy<TestModel, ChainEvent> = Syzygy::builder()
        .model(TestModel { counter: 0, name: "test".to_string() })
        .build();
    
    // Start chain
    syzygy.dispatch(ChainEvent);
    
    // Process all - should stop at counter = 5
    syzygy.handle_effects();
    
    assert_eq!(syzygy.model().counter, 5);
}

#[test]
fn test_mixed_event_types() {
    // Test with enum events to handle multiple types
    #[derive(Debug)]
    enum TestEvent {
        Increment(IncrementEvent),
        SetName(SetNameEvent),
    }
    
    impl Event<TestModel> for TestEvent {
        fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
            match self {
                TestEvent::Increment(e) => {
                    syzygy.model_mut().counter += e.amount;
                }
                TestEvent::SetName(e) => {
                    syzygy.model_mut().name = e.name;
                }
            }
        }
    }
    
    let mut syzygy: Syzygy<TestModel, TestEvent> = Syzygy::builder()
        .model(TestModel { counter: 0, name: "test".to_string() })
        .build();
    
    syzygy.dispatch(TestEvent::Increment(IncrementEvent { amount: 10 }));
    syzygy.dispatch(TestEvent::SetName(SetNameEvent { name: "updated".to_string() }));
    
    syzygy.handle_effects();
    
    assert_eq!(syzygy.model().counter, 10);
    assert_eq!(syzygy.model().name, "updated");
}

#[test]
fn test_controlled_event_chains() {
    // Test that shows event chains work but can be controlled
    #[derive(Debug)]
    struct ControlledEvent { max: i32 }
    
    impl Event<TestModel> for ControlledEvent {
        fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
            syzygy.model_mut().counter += 1;
            if syzygy.model().counter < self.max {
                syzygy.dispatch(ControlledEvent { max: self.max });
            }
        }
    }
    
    let mut syzygy: Syzygy<TestModel, ControlledEvent> = Syzygy::builder()
        .model(TestModel { counter: 0, name: "test".to_string() })
        .build();
    
    syzygy.dispatch(ControlledEvent { max: 10 });
    
    // Process all effects - will stop at counter = 10
    syzygy.handle_effects();
    assert_eq!(syzygy.model().counter, 10);
}

#[test]
fn test_depth_limited_processing() {
    #[derive(Debug)]
    struct DeepChainEvent {
        depth: usize,
    }
    
    impl Event<TestModel> for DeepChainEvent {
        fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
            syzygy.model_mut().counter += 1;
            
            if self.depth < 2 {  // Reduced depth to avoid too many events  
                // Each event spawns 2 new events at the next depth level
                syzygy.dispatch(DeepChainEvent { depth: self.depth + 1 });
                syzygy.dispatch(DeepChainEvent { depth: self.depth + 1 });
            }
        }
    }
    
    let mut syzygy: Syzygy<TestModel, DeepChainEvent> = Syzygy::builder()
        .model(TestModel { counter: 0, name: "test".to_string() })
        .build();
    
    // Start with depth 0
    syzygy.dispatch(DeepChainEvent { depth: 0 });
    
    // Process all effects with reduced depth
    syzygy.handle_effects();
    
    // Level 0: 1 event
    // Level 1: 2 events  
    // Level 2: 4 events
    // Total: 7 events processed
    assert_eq!(syzygy.model().counter, 7);
}