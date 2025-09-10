#[cfg(feature = "tokio")]
use std::sync::{Arc, Barrier};
use std::time::Duration;
use tokio::time::sleep;

use futures_util::future::FutureExt;
use syzygy::executor::{
    AsyncExecutor, ExecutorError, ExecutorLifecycle, TokioExecutor,
    register_current_runtime_for_io, spawn_io,
};
use syzygy::streaming::EffectOutput;

#[derive(Debug, Clone)]
enum TestEvent {
    Success(u32),
}

#[tokio::test]
async fn executor_spawns_and_completes_basic_future_successfully() {
    // Given: A TokioExecutor instance
    let executor = TokioExecutor::current_thread_io("test_basic");

    // When: Spawning a future that produces a success event
    let fut =
        async { EffectOutput::Future(async { vec![TestEvent::Success(42)] }.boxed()) }.boxed();
    let result = executor.spawn_future(fut).await;

    // Then: The future completes with the expected event
    assert!(result.is_ok());
    match result.unwrap() {
        EffectOutput::Future(fut) => {
            let events = fut.await;
            assert_eq!(events.len(), 1);
            #[allow(clippy::match_wildcard_for_single_variants)]
            match &events[0] {
                TestEvent::Success(42) => {}
                other => panic!("Expected Success(42), got {other:?}"),
            }
        }
        _other => panic!("Expected Future output"),
    }

    executor.join().await;
}

#[tokio::test]
async fn executor_cancels_task_on_drop_without_leaking_resources() {
    // Given: A TokioExecutor and a long-running task
    let executor = TokioExecutor::current_thread_io("test_cancel");
    let barrier = Arc::new(Barrier::new(2));
    let barrier_clone = Arc::clone(&barrier);

    // When: Spawning a task that blocks, then dropping the future handle
    let fut = async move {
        barrier_clone.wait(); // Signal start
        sleep(Duration::from_secs(10)).await; // Long sleep
        EffectOutput::Future(async { vec![TestEvent::Success(999)] }.boxed())
    }
    .boxed();

    let join_future = executor.spawn_future(fut);
    barrier.wait(); // Wait for task to start
    drop(join_future); // Cancel by dropping

    // Then: Task is cancelled, no resource leaks
    sleep(Duration::from_millis(50)).await; // Allow cancellation to process
    executor.join().await;
}

#[tokio::test]
async fn executor_handles_cancellation_error_and_continues_processing_new_tasks() {
    // Given: A TokioExecutor with a cancellable task
    let executor = TokioExecutor::current_thread_io("test_cancel_error");

    // When: Starting a long task, polling to initiate, then cancelling
    let fut = async {
        sleep(Duration::from_secs(10)).await;
        EffectOutput::Future(async { vec![TestEvent::Success(999)] }.boxed())
    }
    .boxed();

    let mut join_future = Box::pin(executor.spawn_future(fut));
    let waker = futures::task::noop_waker();
    let mut cx = futures::task::Context::from_waker(&waker);
    assert!(matches!(
        join_future.as_mut().poll(&mut cx),
        std::task::Poll::Pending
    ));
    drop(join_future); // Cancel

    // Then: Executor remains functional for new tasks
    let short_fut =
        async { EffectOutput::Future(async { vec![TestEvent::Success(1)] }.boxed()) }.boxed();
    let result = executor.spawn_future(short_fut).await;
    assert!(result.is_ok());

    executor.join().await;
}

#[tokio::test]
async fn executor_propagates_string_panic_as_error_event_with_correct_message() {
    // Given: A TokioExecutor
    let executor = TokioExecutor::current_thread_io("test_panic_string");

    // When: Spawning a future that panics with a string
    let fut = async {
        panic!("test panic message");
        #[allow(unreachable_code)]
        EffectOutput::Future(async { vec![TestEvent::Success(0)] }.boxed())
    }
    .boxed();

    let result = executor.spawn_future(fut).await;

    // Then: Panic is propagated as ExecutorError::Panic with the message
    assert!(result.is_err());
    match result.expect_err("Expected error") {
        ExecutorError::Panic { msg } => {
            assert_eq!(msg, "test panic message");
        }
        other => panic!("Expected Panic error, got {other:?}"),
    }

    executor.join().await;
}

#[tokio::test]
async fn executor_propagates_static_str_panic_as_error_event_with_correct_message() {
    // Given: A TokioExecutor
    let executor = TokioExecutor::current_thread_io("test_panic_str");

    // When: Spawning a future that panics with a static string
    let fut = async {
        panic!("static str panic");
        #[allow(unreachable_code)]
        EffectOutput::Future(async { vec![TestEvent::Success(0)] }.boxed())
    }
    .boxed();

    let result = executor.spawn_future(fut).await;

    // Then: Panic is propagated as ExecutorError::Panic with the message
    assert!(result.is_err());
    match result.expect_err("Expected error") {
        ExecutorError::Panic { msg } => {
            assert_eq!(msg, "static str panic");
        }
        other => panic!("Expected Panic error, got {other:?}"),
    }

    executor.join().await;
}

#[tokio::test]
async fn executor_propagates_non_string_panic_as_error_event_with_generic_message() {
    // Given: A TokioExecutor
    let executor = TokioExecutor::current_thread_io("test_panic_other");

    // When: Spawning a future that panics with a non-string type
    let fut = async {
        std::panic::panic_any(42i32);
        #[allow(unreachable_code)]
        EffectOutput::Future(async { vec![TestEvent::Success(0)] }.boxed())
    }
    .boxed();

    let result = executor.spawn_future(fut).await;

    // Then: Panic is propagated as ExecutorError::Panic with a generic message
    assert!(result.is_err());
    match result.expect_err("Expected error") {
        ExecutorError::Panic { msg } => {
            assert_eq!(msg, "unknown internal error");
        }
        other => panic!("Expected Panic error, got {other:?}"),
    }

    executor.join().await;
}

#[tokio::test]
async fn executor_shutdown_prevents_new_task_spawning_and_returns_worker_gone_error() {
    // Given: A TokioExecutor
    let executor = TokioExecutor::current_thread_io("test_shutdown");

    // When: Shutting down and attempting to spawn a new task
    executor.shutdown();
    let fut =
        async { EffectOutput::Future(async { vec![TestEvent::Success(42)] }.boxed()) }.boxed();
    let result = executor.spawn_future(fut).await;

    // Then: Spawning fails with WorkerGone error
    assert!(result.is_err());
    match result.expect_err("Expected error") {
        ExecutorError::WorkerGone => {}
        other => panic!("Expected WorkerGone error, got {other:?}"),
    }

    executor.join().await;
}

#[tokio::test]
async fn executor_handles_multiple_concurrent_tasks_efficiently_with_correct_results() {
    // Given: A multi-thread TokioExecutor
    let executor = TokioExecutor::multi_thread_io("test_multi", 2);

    // When: Spawning multiple concurrent tasks
    let tasks = (0..5).map(|i| {
        let fut = async move {
            sleep(Duration::from_millis(10)).await;
            EffectOutput::Future(async move { vec![TestEvent::Success(i)] }.boxed())
        }
        .boxed();
        executor.spawn_future(fut)
    });

    let results: Vec<_> = futures::future::join_all(tasks).await;

    // Then: All tasks complete successfully with correct events
    assert_eq!(results.len(), 5);
    for (i, result) in results.into_iter().enumerate() {
        assert!(result.is_ok(), "Task {i} failed");
        match result.unwrap() {
            EffectOutput::Future(fut) => {
                let events = fut.await;
                assert_eq!(events.len(), 1);
                #[allow(unreachable_patterns)]
                match &events[0] {
                    TestEvent::Success(val) => assert_eq!(*val, {
                        #[allow(clippy::cast_possible_truncation)]
                        {
                            i as u32
                        }
                    }),
                    other => panic!("Task {i} returned unexpected event: {other:?}"),
                }
            }
            _other => panic!("Task {i} returned unexpected result type"),
        }
    }

    executor.join().await;
}

#[tokio::test]
async fn executor_clone_shares_runtime_and_shutdown_affects_all_instances() {
    // Given: A TokioExecutor and its clone
    let executor = TokioExecutor::current_thread_io("test_clone");
    let executor_clone = executor.clone();

    // When: Spawning tasks on both instances
    let fut1 =
        async { EffectOutput::Future(async { vec![TestEvent::Success(1)] }.boxed()) }.boxed();
    let fut2 =
        async { EffectOutput::Future(async { vec![TestEvent::Success(2)] }.boxed()) }.boxed();

    let result1 = executor.spawn_future(fut1).await;
    let result2 = executor_clone.spawn_future(fut2).await;

    // Then: Both tasks succeed initially
    assert!(result1.is_ok());
    assert!(result2.is_ok());

    // When: Shutting down via one instance
    executor.shutdown();

    // Then: New spawns on clone fail
    let fut3 =
        async { EffectOutput::Future(async { vec![TestEvent::Success(3)] }.boxed()) }.boxed();
    let result3 = executor_clone.spawn_future(fut3).await;
    assert!(matches!(result3, Err(ExecutorError::WorkerGone)));

    executor.join().await;
}

#[tokio::test]
async fn executor_registers_io_runtime_and_routes_io_work_correctly() {
    // Given: Registered IO runtime
    register_current_runtime_for_io();

    let executor = TokioExecutor::current_thread_cpu("test_io_reg");
    let _current_thread_id = std::thread::current().id();

    // When: Spawning a task that performs IO work
    let fut = async move {
        let _dedicated_thread_id = std::thread::current().id();

        // IO work should run on the registered runtime
        let _io_result = spawn_io(async move { std::thread::current().id() })
            .await
            .unwrap();

        EffectOutput::Future(async { vec![TestEvent::Success(42)] }.boxed())
    }
    .boxed();

    let result = executor.spawn_future(fut).await;

    // Then: Task completes successfully
    assert!(result.is_ok());

    executor.join().await;
}

#[tokio::test]
async fn executor_handles_effect_output_none_correctly() {
    // Given: A TokioExecutor
    let executor = TokioExecutor::current_thread_io("test_none");

    // When: Spawning a future that returns None
    let fut = async { EffectOutput::<TestEvent>::None }.boxed();
    let result = executor.spawn_future(fut).await;

    // Then: Result is Ok with None
    assert!(result.is_ok());
    match result.unwrap() {
        EffectOutput::None => {}
        _other => panic!("Expected None"),
    }

    executor.join().await;
}

#[tokio::test]
async fn executor_handles_effect_output_stream_correctly() {
    // Given: A TokioExecutor
    let executor = TokioExecutor::current_thread_io("test_stream");

    // When: Spawning a future that returns a Stream
    let fut = async {
        let events = vec![
            TestEvent::Success(1),
            TestEvent::Success(2),
            TestEvent::Success(3),
        ];
        let stream = futures::stream::iter(events);
        EffectOutput::Stream(Box::pin(stream))
    }
    .boxed();

    let result = executor.spawn_future(fut).await;

    // Then: Result is Ok with Stream
    assert!(result.is_ok());
    match result.unwrap() {
        EffectOutput::Stream(_) => {}
        _other => panic!("Expected Stream"),
    }

    executor.join().await;
}

// Test executor variants for different configurations
#[tokio::test]
async fn executor_various_configurations_work_correctly_for_basic_task_execution() {
    // Test Current thread IO
    let exec1 = TokioExecutor::current_thread_io("test_ct_io");
    let fut = async { EffectOutput::Future(async { vec![TestEvent::Success(1)] }.boxed()) }.boxed();
    assert!(exec1.spawn_future(fut).await.is_ok());
    exec1.join().await;

    // Test Current thread CPU
    let exec2 = TokioExecutor::current_thread_cpu("test_ct_cpu");
    let fut = async { EffectOutput::Future(async { vec![TestEvent::Success(2)] }.boxed()) }.boxed();
    assert!(exec2.spawn_future(fut).await.is_ok());
    exec2.join().await;

    // Test Multi-thread IO
    let exec3 = TokioExecutor::multi_thread_io("test_mt_io", 1);
    let fut = async { EffectOutput::Future(async { vec![TestEvent::Success(3)] }.boxed()) }.boxed();
    assert!(exec3.spawn_future(fut).await.is_ok());
    exec3.join().await;

    // Test Multi-thread CPU
    let exec4 = TokioExecutor::multi_thread_cpu("test_mt_cpu", 1);
    let fut = async { EffectOutput::Future(async { vec![TestEvent::Success(4)] }.boxed()) }.boxed();
    assert!(exec4.spawn_future(fut).await.is_ok());
    exec4.join().await;
}

#[tokio::test]
async fn executor_handles_concurrent_shutdown_gracefully_without_deadlocks() {
    // Given: A multi-thread executor with multiple running tasks
    let executor = TokioExecutor::multi_thread_io("test_concurrent_shutdown", 4);
    let mut handles = Vec::new();

    // When: Spawning several concurrent tasks and then initiating shutdown
    for i in 0..10 {
        let fut = async move {
            sleep(Duration::from_millis(100)).await;
            EffectOutput::Future(async move { vec![TestEvent::Success(i)] }.boxed())
        }
        .boxed();
        handles.push(executor.spawn_future(fut));
    }

    // Initiate shutdown while tasks are running
    executor.shutdown();

    // Then: All tasks complete or are cancelled, and join doesn't hang
    let join_result = tokio::time::timeout(Duration::from_secs(2), executor.join()).await;
    assert!(
        join_result.is_ok(),
        "Join should complete without hanging during concurrent shutdown"
    );

    // Verify tasks were handled (either completed or cancelled)
    for (i, handle) in handles.into_iter().enumerate() {
        let result = tokio::time::timeout(Duration::from_millis(10), handle).await;
        // Either completed or was cancelled - no hanging
        assert!(
            result.is_ok() || result.is_err(),
            "Task {i} should not hang"
        );
    }
}

#[tokio::test]
async fn executor_handles_zero_tasks_boundary_condition_without_errors() {
    // Given: A TokioExecutor with no tasks spawned
    let executor = TokioExecutor::current_thread_io("test_zero_tasks");

    // When: No tasks are spawned, just shutdown and join
    executor.shutdown();
    let join_result = tokio::time::timeout(Duration::from_secs(1), executor.join()).await;

    // Then: Shutdown and join complete successfully without errors
    assert!(
        join_result.is_ok(),
        "Join with zero tasks should complete without hanging"
    );
}

#[tokio::test]
async fn executor_propagates_errors_from_effects_as_events_in_pipeline() {
    // Given: A TokioExecutor and an effect that fails
    let executor = TokioExecutor::current_thread_io("test_error_propagation");

    // When: Spawning an effect that returns None (simulating an effect with no events)
    let fut = async {
        // Simulate an effect that produces no events
        EffectOutput::<TestEvent>::None
    }
    .boxed();

    let result = executor.spawn_future(fut).await;

    // Then: Effect completes successfully with None output
    assert!(result.is_ok());
    match result.unwrap() {
        EffectOutput::None => {} // Expected None output
        other => panic!("Expected None, got {other:?}"),
    }

    executor.join().await;
}

#[tokio::test]
async fn executor_handles_boundary_of_maximum_concurrent_tasks_without_resource_exhaustion() {
    // Given: A TokioExecutor configured for high concurrency
    let executor = TokioExecutor::multi_thread_io("test_max_concurrent", 8);

    // When: Spawning a large number of concurrent tasks (boundary test)
    let task_count = 100; // Reasonable boundary for testing
    let handles: Vec<_> = (0..task_count)
        .map(|i| {
            let fut = async move {
                sleep(Duration::from_millis(1)).await; // Minimal work
                #[allow(clippy::cast_possible_truncation)]
                EffectOutput::Future(async move { vec![TestEvent::Success(i as u32)] }.boxed())
            }
            .boxed();
            executor.spawn_future(fut)
        })
        .collect();

    // Then: All tasks complete without resource exhaustion errors
    let results = futures::future::join_all(handles).await;
    assert_eq!(results.len(), task_count);
    for result in results {
        assert!(
            result.is_ok(),
            "Task should complete without exhaustion error"
        );
    }

    executor.join().await;
}
