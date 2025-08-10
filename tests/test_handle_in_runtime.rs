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
    
    #[derive(Debug)]
    struct SpawnTaskEvent {
        flag: Arc<AtomicBool>,
    }
    
    impl Event<TestModel> for SpawnTaskEvent {
        fn apply(self, ctx: &mut Syzygy<TestModel, Self>) {
            println!("Effect running, spawning task...");
            let flag = self.flag.clone();
            ctx.task(move |_snapshot| async move {
                println!("Task running!");
                flag.store(true, Ordering::SeqCst);
            });
            println!("Task spawned from effect");
        }
    }
    
    let flag = Arc::new(AtomicBool::new(false));
    let mut syzygy: Syzygy<TestModel, SpawnTaskEvent> = Syzygy::builder()
        .model(TestModel { value: 0 })
        .build();

    // Dispatch an event that spawns a task
    syzygy.dispatch(SpawnTaskEvent { flag: flag.clone() });
    
    // Process the effect
    syzygy.handle_effects();
    
    // Yield to let the runtime poll the task
    tokio::task::yield_now().await;
    sleep(Duration::from_millis(100)).await;
    
    assert!(flag.load(Ordering::SeqCst), "Task should have run");
}