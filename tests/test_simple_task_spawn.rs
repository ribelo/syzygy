#[cfg(feature = "async")]
mod simple_task_tests {
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

    #[tokio::test]
    async fn test_basic_task_spawn() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut syzygy = Syzygy::builder()
            .model(TestModel { value: 0 })
            .build();

        let flag_clone = flag.clone();
        
        // Dispatch a task effect
        syzygy.task(move |_ctx| async move {
            flag_clone.store(true, Ordering::SeqCst);
        });
        
        // Process the effect to actually spawn the task
        syzygy.handle_effects();
        
        // Give task time to run
        sleep(Duration::from_millis(100)).await;
        
        let result = flag.load(Ordering::SeqCst);
        println!("Flag value: {}", result);
        assert!(result, "Task should have run");
    }
}