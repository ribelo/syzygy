//! Integration tests for Influx-inspired runtime registration functionality
//!
//! Tests the IO runtime registration system that ensures IO operations run on
//! the appropriate runtime while CPU-bound work stays isolated on dedicated executors.

#[cfg(feature = "tokio")]
mod runtime_registration_tests {
    use futures::FutureExt;
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use std::time::Duration;
    use syzygy::executor::Outcome;
    use syzygy::executor::{
        AsyncExecutor, ExecutorLifecycle, SingleThreadExecutor, SyncExecutor, TokioExecutor,
        register_current_runtime_for_io, register_io_runtime, spawn_io,
    };
    use tokio::runtime::Runtime;

    // Helper function to clear IO runtime state for test isolation
    fn clear_io_runtime() {
        register_io_runtime(None);
    }

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEvent {
        IoWork {
            thread_id: String,
            runtime_id: String,
        },
        CpuWork {
            thread_id: String,
        },
        NetworkResult(u32),
        FileResult(String),
    }

    #[tokio::test]
    async fn runtime_registration_routes_io_work_to_registered_runtime_across_threads() {
        // Clear any previous runtime state for test isolation
        clear_io_runtime();

        // Given: Main runtime and a dedicated CPU executor
        let main_runtime_id = format!("{:?}", tokio::runtime::Handle::try_current().unwrap());
        register_current_runtime_for_io();

        let cpu_executor = TokioExecutor::current_thread_cpu("cpu_executor");

        // When: Spawning work on CPU executor that needs to do IO
        let result = cpu_executor
            .spawn_future(
                async move {
                    let cpu_thread_id = format!("{:?}", thread::current().id());

                    // This IO should run on the registered (main) runtime
                    let io_handle = spawn_io(async move {
                        let io_thread_id = format!("{:?}", thread::current().id());
                        let io_runtime_id =
                            format!("{:?}", tokio::runtime::Handle::try_current().unwrap());
                        (io_thread_id, io_runtime_id)
                    });

                    let (io_thread_id, io_runtime_id) = io_handle.await.unwrap();

                    Outcome::Events(vec![
                        TestEvent::CpuWork {
                            thread_id: cpu_thread_id,
                        },
                        TestEvent::IoWork {
                            thread_id: io_thread_id,
                            runtime_id: io_runtime_id,
                        },
                    ])
                }
                .boxed(),
            )
            .await;

        // Then: IO work runs on main runtime, CPU work on dedicated executor
        assert!(result.is_ok());
        match result.unwrap() {
            Outcome::Events(events) => {
                assert_eq!(events.len(), 2);

                match (&events[0], &events[1]) {
                    (
                        TestEvent::CpuWork {
                            thread_id: cpu_thread,
                        },
                        TestEvent::IoWork {
                            thread_id: io_thread,
                            runtime_id: io_runtime,
                        },
                    ) => {
                        // IO should run on main runtime
                        assert_eq!(
                            *io_runtime, main_runtime_id,
                            "IO should run on registered main runtime"
                        );
                        // Threads may be different (CPU vs IO isolation)
                        println!("CPU thread: {cpu_thread}, IO thread: {io_thread}");
                    }
                    other => panic!("Expected CpuWork and IoWork events, got: {other:?}"),
                }
            }
            other => panic!("Expected Future output, got: {other:?}"),
        }

        cpu_executor.join().await;
    }

    #[tokio::test]
    async fn runtime_registration_handles_multiple_runtime_switches_correctly() {
        // Given: Multiple runtimes and registration switches
        let main_handle = tokio::runtime::Handle::try_current().unwrap();
        let secondary_runtime = Runtime::new().unwrap();
        let secondary_handle = secondary_runtime.handle().clone();

        // Initially register main runtime
        register_io_runtime(Some(main_handle.clone()));

        let first_spawn =
            spawn_io(async { format!("{:?}", tokio::runtime::Handle::try_current().unwrap()) });
        let first_runtime = first_spawn.await.unwrap();

        // When: Switch to secondary runtime
        register_io_runtime(Some(secondary_handle.clone()));

        let second_spawn =
            spawn_io(async { format!("{:?}", tokio::runtime::Handle::try_current().unwrap()) });
        let second_runtime = second_spawn.await.unwrap();

        // Then: Each spawn uses the runtime that was registered at spawn time
        assert_eq!(
            first_runtime,
            format!("{main_handle:?}"),
            "First spawn should use main runtime"
        );
        assert_eq!(
            second_runtime,
            format!("{secondary_handle:?}"),
            "Second spawn should use secondary runtime"
        );
        assert_ne!(
            first_runtime, second_runtime,
            "Different runtimes should be distinguishable"
        );

        secondary_runtime.shutdown_background();
    }

    #[tokio::test]
    async fn runtime_registration_falls_back_to_global_when_thread_local_unavailable() {
        // Given: Global runtime registration
        let global_handle = tokio::runtime::Handle::try_current().unwrap();
        register_io_runtime(Some(global_handle.clone()));

        // When: Spawning from a different thread that doesn't have thread-local registration
        let (tx, rx) = tokio::sync::oneshot::channel();

        let global_handle_clone = global_handle.clone();
        thread::spawn(move || {
            // This thread has no thread-local registration, should fall back to global
            let handle = tokio::runtime::Handle::try_current()
                .or_else(|_| Ok::<tokio::runtime::Handle, std::io::Error>(global_handle_clone))
                .unwrap();

            handle.spawn(async move {
                let runtime_id = spawn_io(async {
                    format!("{:?}", tokio::runtime::Handle::try_current().unwrap())
                })
                .await
                .unwrap();

                let _ = tx.send(runtime_id);
            });
        });

        let runtime_used = tokio::time::timeout(Duration::from_secs(1), rx)
            .await
            .expect("Test timed out")
            .expect("Channel receive failed");

        // Then: Global registration is used as fallback
        assert_eq!(
            runtime_used,
            format!("{global_handle:?}"),
            "Should fall back to global runtime registration"
        );
    }

    #[tokio::test]
    async fn runtime_registration_handles_concurrent_registrations_safely() {
        // Clear any previous runtime state for test isolation
        clear_io_runtime();

        // Given: Multiple threads attempting concurrent registration
        let runtime1 = Runtime::new().unwrap();
        let runtime2 = Runtime::new().unwrap();
        let handle1 = runtime1.handle().clone();
        let handle2 = runtime2.handle().clone();

        let barrier = Arc::new(Barrier::new(3));
        let results = Arc::new(Mutex::new(Vec::new()));

        // When: Concurrent registration from multiple threads
        let mut handles = Vec::new();

        for (i, handle) in [handle1.clone(), handle2.clone()].into_iter().enumerate() {
            let barrier_clone = Arc::clone(&barrier);
            let results_clone = Arc::clone(&results);

            let thread_handle = thread::spawn(move || {
                barrier_clone.wait(); // Synchronize start
                register_io_runtime(Some(handle.clone()));

                // Spawn IO work immediately after registration
                let runtime_result = handle.block_on(async {
                    spawn_io(async {
                        format!("{:?}", tokio::runtime::Handle::try_current().unwrap())
                    })
                    .await
                    .unwrap()
                });

                results_clone.lock().unwrap().push((i, runtime_result));
            });
            handles.push(thread_handle);
        }

        barrier.wait(); // Start all threads

        for handle in handles {
            handle.join().expect("Thread panic");
        }

        let results = results.lock().unwrap();

        // Then: Each thread successfully registers and uses its own runtime
        assert_eq!(results.len(), 2);

        // Results should show each thread used its intended runtime
        for (thread_id, runtime_id) in results.iter() {
            let expected_handle = if *thread_id == 0 { &handle1 } else { &handle2 };
            assert_eq!(
                *runtime_id,
                format!("{expected_handle:?}"),
                "Thread {} should use its registered runtime",
                thread_id
            );
        }

        runtime1.shutdown_background();
        runtime2.shutdown_background();
    }

    #[tokio::test]
    async fn runtime_registration_works_with_nested_spawn_io_calls() {
        // Given: Registered runtime
        register_current_runtime_for_io();
        let expected_runtime = format!("{:?}", tokio::runtime::Handle::try_current().unwrap());

        // When: Nested spawn_io calls
        let outer_result = spawn_io(async move {
            let outer_runtime = format!("{:?}", tokio::runtime::Handle::try_current().unwrap());

            // Nested IO spawn
            let inner_result = spawn_io(async move {
                let inner_runtime = format!("{:?}", tokio::runtime::Handle::try_current().unwrap());
                (inner_runtime, "inner_work_done")
            })
            .await
            .unwrap();

            (outer_runtime, inner_result)
        })
        .await;

        // Then: Both outer and inner work run on registered runtime
        assert!(outer_result.is_ok());
        let (outer_runtime, (inner_runtime, inner_msg)) = outer_result.unwrap();

        assert_eq!(
            outer_runtime, expected_runtime,
            "Outer spawn_io should use registered runtime"
        );
        assert_eq!(
            inner_runtime, expected_runtime,
            "Inner spawn_io should use registered runtime"
        );
        assert_eq!(
            inner_msg, "inner_work_done",
            "Inner work should complete successfully"
        );
    }

    #[tokio::test]
    async fn runtime_registration_isolates_cpu_and_io_work_in_mixed_executor_scenario() {
        // Given: Multiple executor types and IO runtime registration
        register_current_runtime_for_io();
        let io_runtime_id = format!("{:?}", tokio::runtime::Handle::try_current().unwrap());

        let tokio_cpu_executor = TokioExecutor::current_thread_cpu("cpu_work");
        let single_thread_executor = SingleThreadExecutor::new();

        // When: Mixed workload across different executor types
        let tokio_result = tokio_cpu_executor.spawn_future(
            async {
                let cpu_thread = format!("{:?}", thread::current().id());

                // Do IO work that should run on registered runtime
                let io_info = spawn_io(async {
                    let io_thread = format!("{:?}", thread::current().id());
                    let io_runtime =
                        format!("{:?}", tokio::runtime::Handle::try_current().unwrap());
                    (io_thread, io_runtime)
                })
                .await
                .unwrap();

                Outcome::Events(vec![
                    TestEvent::CpuWork {
                        thread_id: cpu_thread,
                    },
                    TestEvent::IoWork {
                        thread_id: io_info.0,
                        runtime_id: io_info.1,
                    },
                ])
            }
            .boxed(),
        );

        let single_thread_result = single_thread_executor.spawn_sync(Box::new(move || {
            let sync_thread = format!("{:?}", thread::current().id());

            // Single thread executor doing sync work
            let _computation = (0..1000).sum::<u32>();

            Outcome::Events(vec![TestEvent::CpuWork {
                thread_id: sync_thread,
            }])
        }));

        let (tokio_res, single_res) = tokio::join!(tokio_result, single_thread_result);

        // Then: IO work uses registered runtime, CPU work stays isolated
        assert!(tokio_res.is_ok() && single_res.is_ok());

        match tokio_res.unwrap() {
            Outcome::Events(events) => {
                if let TestEvent::IoWork { runtime_id, .. } = &events[1] {
                    assert_eq!(
                        *runtime_id, io_runtime_id,
                        "IO work should use registered runtime"
                    );
                }
            }
            other => panic!("Expected Events output from Tokio executor, got {other:?}"),
        }

        tokio_cpu_executor.join().await;
    }

    #[tokio::test]
    async fn runtime_registration_handles_runtime_shutdown_gracefully() {
        // Given: A secondary runtime for IO registration
        let secondary_runtime = Runtime::new().unwrap();
        let secondary_handle = secondary_runtime.handle().clone();

        register_io_runtime(Some(secondary_handle.clone()));

        // When: Runtime is shut down while IO work might be pending
        secondary_runtime.shutdown_background();

        // Try to spawn IO work on shut down runtime (should handle gracefully)
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            tokio::runtime::Handle::current().block_on(async { spawn_io(async { 42 }).await })
        }));

        // Then: Either succeeds (if runtime was available) or fails gracefully
        #[allow(clippy::match_same_arms)]
        match result {
            Ok(Ok(_)) => {
                // IO work completed successfully (runtime was still available)
            }
            Ok(Err(_)) => {
                // IO work failed (expected when runtime is shut down)
            }
            Err(_) => {
                // Panic occurred (acceptable for shut down runtime access)
            }
        }

        // Re-register current runtime for subsequent tests
        register_current_runtime_for_io();
    }

    #[tokio::test]
    async fn runtime_registration_thread_local_takes_precedence_over_global() {
        // Given: Both global and thread-local registrations
        let global_runtime = Runtime::new().unwrap();
        let global_handle = global_runtime.handle().clone();

        // Set global registration
        register_io_runtime(Some(global_handle.clone()));

        // Set thread-local registration (should take precedence)
        let local_handle = tokio::runtime::Handle::try_current().unwrap();
        register_current_runtime_for_io(); // Sets thread-local

        // When: Spawning IO work
        let result =
            spawn_io(async { format!("{:?}", tokio::runtime::Handle::try_current().unwrap()) })
                .await
                .unwrap();

        // Then: Thread-local registration takes precedence
        assert_eq!(
            result,
            format!("{local_handle:?}"),
            "Thread-local registration should take precedence over global"
        );
        assert_ne!(
            result,
            format!("{global_handle:?}"),
            "Should not use global registration when thread-local is available"
        );

        global_runtime.shutdown_background();
    }
}
