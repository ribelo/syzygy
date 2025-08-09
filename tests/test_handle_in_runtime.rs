#[cfg(feature = "async")]
#[tokio::test]
async fn test_handle_effects_in_runtime() {
    use syzygy::prelude::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::time::sleep;
    use std::time::Duration;

    #[derive(Debug, Clone)]
    struct TestModel {
        value: i32,
    }

    impl Model for TestModel {
        type Snapshot = Self;
        fn to_snapshot(&self) -> Self::Snapshot {
            self.clone()
        }
    }
    
    let flag = Arc::new(AtomicBool::new(false));
    let mut syzygy = Syzygy::builder()
        .model(TestModel { value: 0 })
        .build();

    let flag_clone = flag.clone();
    
    // Dispatch a regular effect that spawns a task
    syzygy.dispatch(move |ctx| {
        println!("Effect running, spawning task...");
        let tracker = ctx.task_tracker.clone();
        tracker.spawn(async move {
            println!("Task running!");
            flag_clone.store(true, Ordering::SeqCst);
        });
        println!("Task spawned from effect");
    });
    
    // Process the effect
    syzygy.handle_effects();
    
    // Yield to let the runtime poll the task
    tokio::task::yield_now().await;
    sleep(Duration::from_millis(100)).await;
    
    assert!(flag.load(Ordering::SeqCst), "Task should have run");
}