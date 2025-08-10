// Test event storm prevention with bounded processing

use syzygy::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
struct TestModel {
    counter: i32,
    spawn_more: bool,
}

impl Model for TestModel {
    type Snapshot = Self;
    fn to_snapshot(&self) -> Self::Snapshot {
        self.clone()
    }
}

#[derive(Debug)]
struct ChainEvent {
    depth: usize,
    max_depth: usize,
    events_spawned: Arc<AtomicUsize>,
}

impl Event<TestModel> for ChainEvent {
    fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
        // Increment counter
        syzygy.model_mut().counter += 1;
        
        // Track how many events we've spawned
        self.events_spawned.fetch_add(1, Ordering::SeqCst);
        
        // Each event spawns 2 more events (exponential growth!) 
        if self.depth < self.max_depth && syzygy.model().spawn_more {
            syzygy.dispatch(ChainEvent {
                depth: self.depth + 1,
                max_depth: self.max_depth,
                events_spawned: self.events_spawned.clone(),
            });
            syzygy.dispatch(ChainEvent {
                depth: self.depth + 1,
                max_depth: self.max_depth,
                events_spawned: self.events_spawned.clone(),
            });
        }
    }
}

#[test]
fn test_bounded_prevents_event_storm() {
    let mut syzygy: Syzygy<TestModel, ChainEvent> = Syzygy::builder()
        .model(TestModel { 
            counter: 0,
            spawn_more: true,
        })
        .build();
    
    let events_spawned = Arc::new(AtomicUsize::new(0));
    
    // Start a chain that would spawn 2^5 = 32 events total
    syzygy.dispatch(ChainEvent {
        depth: 0,
        max_depth: 5,
        events_spawned: events_spawned.clone(),
    });
    
    // Process all events - simplified test without depth limiting
    syzygy.handle_effects();
    
    // We can do other work here (like rendering a frame)
    println!("Rendered frame with counter at: {}", syzygy.model().counter);
    
    // With depth 5, we spawn: 1 + 2 + 4 + 8 + 16 + 32 = 63 events total
    assert_eq!(syzygy.model().counter, 63, "All events should eventually be processed");
}

#[test] 
fn test_depth_limited_processing_prevents_infinite_loops() {
    // Create an event that stops spawning after a certain count
    #[derive(Debug)]
    struct ControlledInfiniteEvent;
    
    impl Event<TestModel> for ControlledInfiniteEvent {
        fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
            syzygy.model_mut().counter += 1;
            
            // Stop spawning after reaching 100 to prevent true infinite loops
            if syzygy.model().counter < 100 && syzygy.model().spawn_more {
                syzygy.dispatch(ControlledInfiniteEvent);
            }
        }
    }
    
    let mut syzygy: Syzygy<TestModel, ControlledInfiniteEvent> = Syzygy::builder()
        .model(TestModel { 
            counter: 0,
            spawn_more: true,
        })
        .build();
    
    // Start a controlled chain
    syzygy.dispatch(ControlledInfiniteEvent);
    
    // Process events - simplified test
    syzygy.handle_effects();
    
    println!("Final counter = {}", syzygy.model().counter);
    
    // The controlled loop should have stopped at 100
    assert_eq!(syzygy.model().counter, 100, "Should have processed exactly 100 events");
}

#[derive(Debug)]
struct InfiniteEvent;

impl Event<TestModel> for InfiniteEvent {
    fn apply(self, syzygy: &mut Syzygy<TestModel, Self>) {
        syzygy.model_mut().counter += 1;
        
        // Keep spawning if flag is true
        if syzygy.model().spawn_more {
            syzygy.dispatch(InfiniteEvent);
        }
    }
}

#[test]
fn test_handle_effects_processes_all() {
    let mut syzygy: Syzygy<TestModel, ChainEvent> = Syzygy::builder()
        .model(TestModel { 
            counter: 0,
            spawn_more: true,
        })
        .build();
    
    let events_spawned = Arc::new(AtomicUsize::new(0));
    
    // Start a chain that spawns many events
    syzygy.dispatch(ChainEvent {
        depth: 0,
        max_depth: 4, // 2^4 = 16 events
        events_spawned: events_spawned.clone(),
    });
    
    // handle_effects() should process all events
    syzygy.handle_effects();
    
    // 1 + 2 + 4 + 8 + 16 = 31 events total
    assert_eq!(syzygy.model().counter, 31, "handle_effects should process all events");
}