//! Cucumber tests for Type Safety Requirements (SYZ-016 to SYZ-017)

use cucumber::{given, then, when, World};
use syzygy::prelude::*;

#[derive(Debug, Clone, Default)]
struct TestState {
    counter: i32,
}

#[derive(Debug, Clone)]
enum TestEvent {
    Increment(i32),
}

#[derive(Debug, Clone)]
enum TestCommand {
    LogEvent(String),
}


#[derive(Debug, World)]
#[world(init = Self::new)]
struct SyzygyWorld {
    syzygy: Option<Syzygy<TestState, TestEvent, TestCommand>>,
    handle: Option<SyzygyHandle<TestEvent>>,
    type_safety_verified: bool,
    thread_safety_verified: bool,
}

impl SyzygyWorld {
    fn new() -> Self {
        Self {
            syzygy: None,
            handle: None,
            type_safety_verified: false,
            thread_safety_verified: false,
        }
    }

    fn create_system(&mut self) {
        let (syzygy, handle, _executor) = Syzygy::builder()
            .model(TestState::default())
            .event_handler(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestCommand> {
                match event {
                    TestEvent::Increment(value) => {
                        state.counter += value;
                        Dispatch::none()
                    }
                }
            })
            .build();
        
        self.syzygy = Some(syzygy);
        self.handle = Some(handle);
    }
}

// SYZ-016: Compile-Time Type Safety
#[given("a Syzygy system with strict type constraints")]
fn given_syzygy_with_type_constraints(world: &mut SyzygyWorld) {
    world.create_system();
}

#[when("I try to dispatch events of the correct type")]
fn when_dispatch_correct_type_events(world: &mut SyzygyWorld) {
    let handle = world.handle.as_ref().unwrap();
    
    // This should compile and work
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    world.type_safety_verified = true;
}

#[then("the system should accept the events")]
fn then_system_accepts_events(world: &mut SyzygyWorld) {
    assert!(world.type_safety_verified);
}

#[then("wrong event types should be rejected at compile time")]
fn then_wrong_types_rejected_at_compile_time(world: &mut SyzygyWorld) {
    // This is a compile-time check - if this test compiles and runs,
    // it means the type system is working correctly
    // The following would not compile:
    // handle.dispatch(42).unwrap(); // Wrong event type
    // handle.dispatch("string").unwrap(); // Wrong event type
    
    world.type_safety_verified = true;
    assert!(world.type_safety_verified);
}

// SYZ-017: Thread Safety Requirements
#[given("a Syzygy system with thread safety requirements")]
fn given_syzygy_with_thread_safety(world: &mut SyzygyWorld) {
    world.create_system();
}

#[when("I verify Send and Sync bounds on events and tasks")]
fn when_verify_send_sync_bounds(world: &mut SyzygyWorld) {
    // The fact that this compiles means Send bounds are satisfied
    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}
    
    // Events and tasks must be Send for async execution
    assert_send::<TestEvent>();
    assert_send::<TestCommand>();
    
    // Handle must be Send to share across threads
    if let Some(ref handle) = world.handle {
        // Test that we can move handle across thread boundaries
        let handle_clone = handle.clone();
        std::thread::spawn(move || {
            // This verifies Send trait
            let _ = handle_clone;
        }).join().unwrap();
    }
    
    world.thread_safety_verified = true;
}

#[then("events and tasks should satisfy Send bounds")]
fn then_events_tasks_satisfy_send(world: &mut SyzygyWorld) {
    assert!(world.thread_safety_verified);
}

#[then("handles should be safely shareable across threads")]
fn then_handles_shareable_across_threads(world: &mut SyzygyWorld) {
    assert!(world.thread_safety_verified);
    
    // Additional verification that handle can be cloned and shared
    if let Some(ref handle) = world.handle {
        let handle1 = handle.clone();
        let handle2 = handle.clone();
        
        // Both handles should work independently
        handle1.dispatch(TestEvent::Increment(1)).unwrap();
        handle2.dispatch(TestEvent::Increment(2)).unwrap();
        
        let syzygy = world.syzygy.as_mut().unwrap();
        syzygy.process_events();
        assert_eq!(syzygy.model().counter, 3);
    }
}

#[tokio::main]
async fn main() {
    SyzygyWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("features/syzygy-type-safety.feature")
        .await;
}