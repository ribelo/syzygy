#[cfg(feature = "tokio")]
mod tokio_newtype_tests {
    use syzygy::executor::{
        TokioIo, TokioCpu, ExecutorRegistry, AsyncExecutor, ExecutorLifecycle
    };
    use syzygy::streaming::EffectOutput;
    use futures_util::future::FutureExt;
    use std::any::TypeId;

    #[derive(Debug, Clone)]
    enum TestEvent {
        Done,
        Progress(u32),
    }

    #[test]
    fn tokio_io_and_tokio_cpu_have_distinct_type_ids() {
        // Given: TokioIo and TokioCpu types
        let io_type_id = TypeId::of::<TokioIo>();
        let cpu_type_id = TypeId::of::<TokioCpu>();

        // Then: They have different TypeIds
        assert_ne!(io_type_id, cpu_type_id, "TokioIo and TokioCpu must have distinct TypeIds");
    }

    #[test]
    fn both_newtypes_can_be_registered_in_same_registry() {
        // Given: A registry and both executor types
        let mut registry = ExecutorRegistry::<TestEvent>::new();
        let io_exec = TokioIo::current_thread();
        let cpu_exec = TokioCpu::current_thread();

        // When: Both are registered
        registry.insert_async(io_exec);
        registry.insert_async(cpu_exec);

        // Then: Both can be retrieved by their types
        assert!(registry.async_exec::<TokioIo>().is_some(), "TokioIo should be in registry");
        assert!(registry.async_exec::<TokioCpu>().is_some(), "TokioCpu should be in registry");
    }

    #[tokio::test]
    async fn tokio_io_executes_futures_correctly() {
        // Given: A TokioIo executor
        let executor = TokioIo::default();

        // When: Spawning a future
        let fut = async {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            EffectOutput::Future(async { vec![TestEvent::Done] }.boxed())
        }.boxed();

        let result = executor.spawn_future(fut).await;

        // Then: Future executes successfully
        assert!(result.is_ok(), "TokioIo should execute futures");
        match result.unwrap() {
            EffectOutput::Future(events_future) => {
                let events = events_future.await;
                assert_eq!(events.len(), 1);
                matches!(events[0], TestEvent::Done);
            }
            _ => panic!("Expected Future output"),
        }
    }

    #[tokio::test]
    async fn tokio_cpu_executes_futures_correctly() {
        // Given: A TokioCpu executor
        let executor = TokioCpu::default();

        // When: Spawning a compute-heavy future
        let fut = async {
            // Simulate CPU work
            let mut sum = 0u64;
            for i in 0..1000 {
                sum = sum.wrapping_add(i);
            }
            EffectOutput::Future(async move { vec![TestEvent::Progress(sum as u32)] }.boxed())
        }.boxed();

        let result = executor.spawn_future(fut).await;

        // Then: Future executes successfully
        assert!(result.is_ok(), "TokioCpu should execute futures");
        match result.unwrap() {
            EffectOutput::Future(events_future) => {
                let events = events_future.await;
                assert_eq!(events.len(), 1);
                matches!(events[0], TestEvent::Progress(_));
            }
            _ => panic!("Expected Future output"),
        }
    }

    #[tokio::test]
    async fn tokio_io_multi_thread_constructor_works() {
        // Given: Multi-thread TokioIo with 2 threads
        let executor = TokioIo::multi_thread(2);

        // When: Spawning work
        let fut = async {
            EffectOutput::<TestEvent>::None
        }.boxed();

        let result = executor.spawn_future(fut).await;

        // Then: Works correctly
        assert!(result.is_ok());
        match result.unwrap() {
            EffectOutput::None => {
                // Multi-thread executors may return None for empty work
            }
            EffectOutput::Future(events_future) => {
                let events = events_future.await;
                assert_eq!(events.len(), 0);
            }
            _ => panic!("Unexpected output"),
        }
    }

    #[tokio::test]
    async fn tokio_cpu_multi_thread_constructor_works() {
        // Given: Multi-thread TokioCpu with 2 threads
        let executor = TokioCpu::multi_thread(2);

        // When: Spawning work
        let fut = async {
            EffectOutput::<TestEvent>::None
        }.boxed();

        let result = executor.spawn_future(fut).await;

        // Then: Works correctly
        assert!(result.is_ok());
        match result.unwrap() {
            EffectOutput::None => {
                // Multi-thread executors may return None for empty work
            }
            EffectOutput::Future(events_future) => {
                let events = events_future.await;
                assert_eq!(events.len(), 0);
            }
            _ => panic!("Unexpected output"),
        }
    }

    #[tokio::test]
    async fn executors_shutdown_gracefully() {
        // Given: Both executor types
        let io_exec = TokioIo::current_thread();
        let cpu_exec = TokioCpu::current_thread();

        // When: Shutting down
        io_exec.shutdown();
        cpu_exec.shutdown();

        // Then: Join completes
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            async {
                io_exec.join().await;
                cpu_exec.join().await;
            }
        ).await.expect("Executors should shutdown within timeout");
    }


}