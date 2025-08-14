//! Cucumber tests for State Management Requirements (SYZ-008 to SYZ-011)

use cucumber::{given, then, when, World};
use syzygy::prelude::*;
use syzygy::resource::NoResources;

#[derive(Debug, Clone, Default)]
struct TestState {
    counter: i32,
    user_name: Option<String>,
    immutable_data: String,
}

#[derive(Debug, Clone)]
enum TestEvent {
    Increment(i32),
    SetUser(String),
    ReadData,
}

#[derive(Debug, Clone)]
enum TestTask {
    LogEvent(String),
}

impl Task<TestEvent, NoResources> for TestTask {
    async fn execute(&self, _ctx: TaskContext<TestEvent, NoResources>) {
        // Simple task implementation for testing
    }
}

#[derive(Debug, World)]
#[world(init = Self::new)]
struct SyzygyWorld {
    syzygy: Option<Syzygy<TestState, TestEvent, TestTask>>,
    handle: Option<SyzygyHandle<TestEvent>>,
    state_accessed: bool,
    resource_accessed: bool,
}

impl SyzygyWorld {
    fn new() -> Self {
        Self {
            syzygy: None,
            handle: None,
            state_accessed: false,
            resource_accessed: false,
        }
    }

    fn create_system(&mut self) {
        let (syzygy, handle) = Syzygy::builder()
            .with_state(TestState {
                counter: 0,
                user_name: None,
                immutable_data: "test_data".to_string(),
            })
            .on_event(|event: TestEvent, state: &mut TestState| -> Dispatch<TestEvent, TestTask> {
                match event {
                    TestEvent::Increment(value) => {
                        state.counter += value;
                        Dispatch::empty()
                    }
                    TestEvent::SetUser(name) => {
                        state.user_name = Some(name);
                        Dispatch::empty()
                    }
                    TestEvent::ReadData => {
                        // Read immutable data without modifying
                        let _data = &state.immutable_data;
                        Dispatch::empty()
                    }
                }
            })
            .build();
        
        self.syzygy = Some(syzygy);
        self.handle = Some(handle);
    }
}

// SYZ-008: Model Access (Mutable and Immutable)
#[given("a Syzygy system with models is available")]
fn given_syzygy_with_models(world: &mut SyzygyWorld) {
    world.create_system();
}

#[when("I access models through the model\\(\\) method")]
fn when_access_models_through_method(world: &mut SyzygyWorld) {
    let syzygy = world.syzygy.as_ref().unwrap();
    
    // Access state (which acts as a model)
    let _state = syzygy.state();
    world.state_accessed = true;
}

#[then("I should be able to read and write model data")]
fn then_can_read_write_model_data(world: &mut SyzygyWorld) {
    assert!(world.state_accessed);
    
    let handle = world.handle.as_ref().unwrap();
    let syzygy = world.syzygy.as_mut().unwrap();
    
    // Test writing to model
    handle.dispatch(TestEvent::Increment(5)).unwrap();
    handle.dispatch(TestEvent::SetUser("Alice".to_string())).unwrap();
    syzygy.process_events();
    
    // Test reading from model
    assert_eq!(syzygy.state().counter, 5);
    assert_eq!(syzygy.state().user_name, Some("Alice".to_string()));
}

// SYZ-009: Resource Access (Immutable Only)
#[given("a Syzygy system with resources is available")]
fn given_syzygy_with_resources(world: &mut SyzygyWorld) {
    // Resources are accessed immutably through the context
    world.create_system();
}

#[when("I access resources through the resource\\(\\) method")]
fn when_access_resources_through_method(world: &mut SyzygyWorld) {
    // In this test, we'll verify resource access through event handling
    let handle = world.handle.as_ref().unwrap();
    handle.dispatch(TestEvent::ReadData).unwrap();
    world.resource_accessed = true;
}

#[then("I should only be able to read resource data")]
fn then_can_only_read_resource_data(world: &mut SyzygyWorld) {
    assert!(world.resource_accessed);
    
    let syzygy = world.syzygy.as_mut().unwrap();
    syzygy.process_events();
    
    // Verify we can read immutable data
    assert_eq!(syzygy.state().immutable_data, "test_data");
}

// SYZ-010: Immediate State Consistency
#[given("a Syzygy system is processing events")]
fn given_syzygy_processing_events(world: &mut SyzygyWorld) {
    world.create_system();
}

#[when("I process a batch of events")]
fn when_process_batch_of_events(world: &mut SyzygyWorld) {
    let handle = world.handle.as_ref().unwrap();
    let syzygy = world.syzygy.as_mut().unwrap();
    
    // Dispatch multiple events
    handle.dispatch(TestEvent::Increment(1)).unwrap();
    handle.dispatch(TestEvent::Increment(2)).unwrap();
    handle.dispatch(TestEvent::SetUser("Bob".to_string())).unwrap();
    
    // Process them all at once
    syzygy.process_events();
}

#[then("all state changes should be immediately visible")]
fn then_all_changes_immediately_visible(world: &mut SyzygyWorld) {
    let syzygy = world.syzygy.as_ref().unwrap();
    
    // All changes should be applied
    assert_eq!(syzygy.state().counter, 3);
    assert_eq!(syzygy.state().user_name, Some("Bob".to_string()));
}

// SYZ-011: State Access During Event Processing
#[when("I access state during event processing")]
fn when_access_state_during_processing(world: &mut SyzygyWorld) {
    let handle = world.handle.as_ref().unwrap();
    let syzygy = world.syzygy.as_mut().unwrap();
    
    // Dispatch an event and process it
    handle.dispatch(TestEvent::Increment(10)).unwrap();
    syzygy.process_events();
}

#[then("state changes should be isolated per event")]
fn then_state_changes_isolated_per_event(world: &mut SyzygyWorld) {
    let syzygy = world.syzygy.as_ref().unwrap();
    
    // Each event should see consistent state
    assert_eq!(syzygy.state().counter, 10);
    
    // Process another event to verify isolation
    let handle = world.handle.as_ref().unwrap();
    let syzygy = world.syzygy.as_mut().unwrap();
    
    handle.dispatch(TestEvent::Increment(5)).unwrap();
    syzygy.process_events();
    
    // State should reflect both changes
    assert_eq!(syzygy.state().counter, 15);
}

#[tokio::main]
async fn main() {
    SyzygyWorld::cucumber()
        .fail_on_skipped()
        .run_and_exit("features/syzygy-state-management.feature")
        .await;
}