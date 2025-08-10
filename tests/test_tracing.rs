//! Test tracing integration
//!
//! This test is compile-only - it verifies that tracing works when enabled
//! and doesn't interfere when disabled.

#[cfg(all(feature = "async", feature = "tracing"))]
mod tracing_tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
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
    
    #[derive(Debug)]
    struct AddEvent { amount: i32 }
    
    impl Event<TestModel> for AddEvent {
        fn apply(self, ctx: &mut Syzygy<TestModel, Self>) {
            ctx.update(|model| model.value += self.amount);
        }
    }

    #[tokio::test]
    async fn test_tracing_compiles() {
        // This test just verifies that tracing integration compiles
        // and doesn't panic when used
        let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
            .model(TestModel { value: 0 })
            .build();

        // Test anonymous task with tracing
        let task_ran = Arc::new(AtomicBool::new(false));
        let ran = task_ran.clone();
        syzygy.task(move |_ctx| async move {
            ran.store(true, Ordering::SeqCst);
        });

        // Test named task with tracing
        syzygy.task_named("test:named_task", |_ctx| async move {
            // Task with tracing span
        });

        // Process effects (should create spans)
        syzygy.handle_effects();
        
        // Wait for tasks
        sleep(Duration::from_millis(50)).await;
        
        assert!(task_ran.load(Ordering::SeqCst), "Task should have run");
    }

    #[tokio::test]
    async fn test_multiple_effects_with_tracing() {
        let mut syzygy: Syzygy<TestModel, AddEvent> = Syzygy::builder()
            .model(TestModel { value: 0 })
            .build();

        // Dispatch multiple effects - each should get a span
        for i in 0..5 {
            syzygy.dispatch(AddEvent { amount: i });
        }

        // Handle all effects - should create handle_effects span
        // with nested effect spans
        syzygy.handle_effects();
        
        assert_eq!(syzygy.model().value, 10); // 0+1+2+3+4
    }
}

#[cfg(all(feature = "async", not(feature = "tracing")))]
mod without_tracing {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };
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
    
    #[derive(Debug)]
    struct AddEvent { amount: i32 }
    
    impl Event<TestModel> for AddEvent {
        fn apply(self, ctx: &mut Syzygy<TestModel, Self>) {
            ctx.update(|model| model.value += self.amount);
        }
    }

    #[tokio::test]
    async fn test_works_without_tracing() {
        // Verify everything still works when tracing is disabled
        let mut syzygy: Syzygy<TestModel, ()> = Syzygy::builder()
            .model(TestModel { value: 0 })
            .build();

        let task_ran = Arc::new(AtomicBool::new(false));
        let ran = task_ran.clone();
        
        syzygy.task(move |_ctx| async move {
            ran.store(true, Ordering::SeqCst);
        });

        syzygy.task_named("test:named", |_ctx| async move {
            // Named task without tracing - should still work
        });

        syzygy.handle_effects();
        sleep(Duration::from_millis(50)).await;
        
        assert!(task_ran.load(Ordering::SeqCst), "Task should have run");
    }
}