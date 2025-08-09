#[cfg(feature = "async")]
mod task_cleanup_tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::Duration;
    use syzygy::prelude::*;
    use tokio::time::sleep;

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
    async fn test_tasks_are_cancelled_on_drop() {
        let task_running = Arc::new(AtomicBool::new(false));
        let task_completed = Arc::new(AtomicBool::new(false));
        
        {
            let mut syzygy = Syzygy::builder()
                .model(TestModel { value: 0 })
                .build();

            let running = task_running.clone();
            let completed = task_completed.clone();
            
            // Spawn a long-running task
            syzygy.task(move |_ctx| async move {
                running.store(true, Ordering::SeqCst);
                // This should be cancelled before completing
                sleep(Duration::from_secs(10)).await;
                completed.store(true, Ordering::SeqCst);
            });
            
            // Process the effect to actually spawn the task
            syzygy.handle_effects();
            
            // Give the task time to start
            sleep(Duration::from_millis(100)).await;
            
            // Verify task started
            assert!(task_running.load(Ordering::SeqCst), "Task should have started");
            
            // Drop will trigger task cancellation
        }
        
        // Give time for cleanup
        sleep(Duration::from_millis(100)).await;
        
        // Task should not have completed
        assert!(!task_completed.load(Ordering::SeqCst), "Task should have been cancelled, not completed");
    }

    #[tokio::test]
    async fn test_graceful_shutdown() {
        let counter = Arc::new(AtomicU32::new(0));
        let mut syzygy = Syzygy::builder()
            .model(TestModel { value: 0 })
            .build();

        // Spawn multiple tasks
        for _ in 0..10 {
            let counter = counter.clone();
            syzygy.task(move |_ctx| async move {
                sleep(Duration::from_millis(50)).await;
                counter.fetch_add(1, Ordering::SeqCst);
            });
        }
        
        // Process all effects to spawn the tasks
        syzygy.handle_effects();

        // Graceful shutdown with timeout
        syzygy.shutdown(Duration::from_secs(1)).await;
        
        // All tasks should have completed
        assert_eq!(counter.load(Ordering::SeqCst), 10, "All tasks should have completed gracefully");
    }

    #[tokio::test]
    async fn test_abort_all_tasks() {
        let task_completed = Arc::new(AtomicBool::new(false));
        let mut syzygy = Syzygy::builder()
            .model(TestModel { value: 0 })
            .build();

        let completed = task_completed.clone();
        syzygy.task(move |_ctx| async move {
            sleep(Duration::from_secs(10)).await;
            completed.store(true, Ordering::SeqCst);
        });
        
        // Process the effect to spawn the task
        syzygy.handle_effects();

        // Give task time to start
        sleep(Duration::from_millis(50)).await;

        // Abort immediately
        syzygy.abort_all_tasks();
        
        // Give time for abort to take effect
        sleep(Duration::from_millis(100)).await;
        
        // Task should not have completed
        assert!(!task_completed.load(Ordering::SeqCst), "Task should have been aborted");
    }

    #[tokio::test]
    async fn test_tasks_cancelled_with_cancellation_token() {
        let before_select = Arc::new(AtomicBool::new(false));
        let after_select = Arc::new(AtomicBool::new(false));
        
        {
            let mut syzygy = Syzygy::builder()
                .model(TestModel { value: 0 })
                .build();

            let before = before_select.clone();
            let after = after_select.clone();
            
            syzygy.task(move |_ctx| async move {
                before.store(true, Ordering::SeqCst);
                // This sleep should be interrupted by cancellation
                sleep(Duration::from_secs(10)).await;
                after.store(true, Ordering::SeqCst);
            });
            
            // Process the effect to spawn the task
            syzygy.handle_effects();
            
            // Let task start
            sleep(Duration::from_millis(100)).await;
            assert!(before_select.load(Ordering::SeqCst), "Task should have started");
            
            // Trigger shutdown
            syzygy.abort_all_tasks();
        }
        
        // Give cleanup time
        sleep(Duration::from_millis(100)).await;
        
        // Task should have been cancelled before completing
        assert!(!after_select.load(Ordering::SeqCst), "Task should have been cancelled");
    }
}