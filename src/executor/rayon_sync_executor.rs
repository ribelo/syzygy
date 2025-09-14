//! `RayonSyncExecutor` — CPU-bound sync executor using Rayon
//!
//! Runs synchronous effect tasks on a dedicated Rayon thread pool. Tasks are
//! closures that return `EffectResult<E>` and are executed on Rayon workers.

use crate::executor::{Concurrent, ExecutorError, Outcome, SyncExecutor};

use futures::channel::oneshot;
use futures_util::future::{BoxFuture, FutureExt, ready};
use rayon::ThreadPoolBuilder;
use std::marker::PhantomData;

/// Rayon-backed executor for synchronous tasks
pub struct RayonExecutor<E>
where
    E: Send + 'static,
{
    pool: rayon::ThreadPool,
    _phantom: PhantomData<E>,
}

impl<E> Concurrent for RayonExecutor<E>
where
    E: Send + Sync + 'static,
{}

impl<E> std::fmt::Debug for RayonExecutor<E>
where
    E: Send + 'static,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RayonSyncExecutor")
    }
}

impl<E> RayonExecutor<E>
where
    E: Send + 'static,
{
    /// Create a new `RayonSyncExecutor` with `n_threads` threads. If `None`, uses available parallelism.
    #[must_use]
    pub fn new(n_threads: Option<usize>) -> Self {
        let n = n_threads
            .or_else(|| {
                std::thread::available_parallelism()
                    .ok()
                    .map(std::num::NonZero::get)
            })
            .unwrap_or(1)
            .max(1);
        let pool = ThreadPoolBuilder::new()
            .num_threads(n)
            .thread_name(|i| format!("syzygy-rayon-{i}"))
            .build()
            .expect("failed to build rayon thread pool");
        Self {
            pool,
            _phantom: PhantomData,
        }
    }
}

impl<E> SyncExecutor<E> for RayonExecutor<E>
where
    E: Send + Sync + 'static,
{
    fn spawn_sync(
        &self,
        job: Box<dyn FnOnce() -> Outcome<E> + Send>,
    ) -> BoxFuture<'static, Result<Outcome<E>, ExecutorError>> {
        let (tx, rx) = oneshot::channel::<Outcome<E>>();
        self.pool.spawn(move || {
            let out = job();
            let _ = tx.send(out);
        });
        async move { rx.await.map_err(|_| ExecutorError::WorkerGone) }.boxed()
    }
}

impl<E> crate::executor::ExecutorLifecycle for RayonExecutor<E>
where
    E: Send + Sync + 'static,
{
    fn shutdown(&self) {}
    fn join(&self) -> BoxFuture<'static, ()> {
        ready(()).boxed()
    }
}

// Executor<E> is object-safe and can be used as `dyn Executor<E>`

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::ExecutorLifecycle;
    use crate::executor::Outcome;
    use std::thread;
    use std::time::{Duration, Instant};

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEvent {
        CpuWork(u32),
        ThreadInfo { thread_id: String, value: u32 },
        Error(String),
    }

    #[test]
    fn rayon_executor_creates_with_specified_thread_count() {
        // Given: Request for specific thread count
        let thread_count = 4;

        // When: Creating RayonSyncExecutor with thread count
        let executor = RayonExecutor::<TestEvent>::new(Some(thread_count));

        // Then: Executor is created successfully
        // Note: We can't directly inspect thread count, but creation should succeed
        assert!(format!("{executor:?}").contains("RayonSyncExecutor"));
    }

    #[test]
    fn rayon_executor_creates_with_default_parallelism_when_none_specified() {
        // Given: No specific thread count
        // When: Creating RayonSyncExecutor with None
        let executor = RayonExecutor::<TestEvent>::new(None);

        // Then: Executor uses available parallelism (at least 1 thread)
        assert!(format!("{executor:?}").contains("RayonSyncExecutor"));
    }

    #[tokio::test]
    async fn rayon_executor_executes_cpu_bound_sync_work_correctly() {
        // Given: RayonSyncExecutor and CPU-intensive work
        let executor = RayonExecutor::<TestEvent>::new(Some(2));

        // When: Spawning CPU-bound synchronous work
        let result = executor
            .spawn_sync(Box::new(|| {
                // Simulate CPU-bound work
                let mut sum = 0u32;
                for i in 0..1000 {
                    sum = sum.wrapping_add(i);
                }
                Outcome::Events(vec![TestEvent::CpuWork(sum)])
            }))
            .await;

        // Then: Work completes successfully with expected result
        assert!(result.is_ok());
        match result.unwrap() {
            Outcome::Events(events) => {
                assert_eq!(events.len(), 1);
                match &events[0] {
                    TestEvent::CpuWork(sum) => assert_eq!(*sum, 499_500), // Sum of 0..1000
                    other => panic!("Expected CpuWork event, got {other:?}"),
                }
            }
            other => panic!("Expected Events output, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rayon_executor_runs_work_on_dedicated_rayon_threads_not_tokio_threads() {
        // Given: RayonSyncExecutor
        let executor = RayonExecutor::<TestEvent>::new(Some(2));

        // When: Spawning work that captures thread information
        let result = executor
            .spawn_sync(Box::new(|| {
                let thread_name = thread::current().name().unwrap_or("unnamed").to_string();

                // Rayon threads should have "syzygy-rayon-" prefix
                let is_rayon_thread = thread_name.starts_with("syzygy-rayon-");
                let value = if is_rayon_thread { 42 } else { 0 };

                Outcome::Events(vec![TestEvent::ThreadInfo {
                    thread_id: thread_name,
                    value,
                }])
            }))
            .await;

        // Then: Work runs on Rayon thread pool
        assert!(result.is_ok());
        match result.unwrap() {
            Outcome::Events(events) => {
                assert_eq!(events.len(), 1);
                match &events[0] {
                    TestEvent::ThreadInfo { thread_id, value } => {
                        assert!(
                            thread_id.starts_with("syzygy-rayon-"),
                            "Expected Rayon thread, got: {thread_id}"
                        );
                        assert_eq!(*value, 42, "Work should run on Rayon thread");
                    }
                    other => panic!("Expected ThreadInfo event, got {other:?}"),
                }
            }
            other => panic!("Expected Events output, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rayon_executor_handles_multiple_concurrent_cpu_tasks_efficiently() {
        // Given: RayonSyncExecutor with multiple threads
        let executor = RayonExecutor::<TestEvent>::new(Some(2));
        let task_count = 10usize;

        // When: Spawning multiple CPU-bound tasks concurrently
        let start_time = Instant::now();
        let mut handles = Vec::new();

        for task_id in 0..task_count {
            let handle = executor.spawn_sync(Box::new(move || {
                // CPU-intensive work with task-specific result
                let mut result = task_id as u64;
                for i in 0..100 {
                    result = result.wrapping_mul(2).wrapping_add(i);
                }
                thread::sleep(Duration::from_millis(10)); // Simulate work time
                #[allow(clippy::cast_possible_truncation)]
                Outcome::Events(vec![TestEvent::CpuWork(result as u32)])
            }));
            handles.push(handle);
        }

        // Wait for all tasks to complete
        let results: Vec<_> = futures::future::join_all(handles).await;
        let total_time = start_time.elapsed();

        // Then: All tasks complete successfully within reasonable time
        assert_eq!(results.len(), task_count);
        for (i, result) in results.into_iter().enumerate() {
            assert!(result.is_ok(), "Task {i} failed");
        }

        // Should complete faster than sequential execution due to parallelism
        assert!(
            total_time < Duration::from_millis(150),
            "Parallel execution took too long: {total_time:?}"
        );
    }

    #[tokio::test]
    #[ignore = "Rayon aborts process on panic - cannot test panic isolation"]
    async fn rayon_executor_isolates_panics_and_returns_worker_gone_error() {
        // Given: RayonSyncExecutor
        let executor = RayonExecutor::<TestEvent>::new(Some(2));

        // When: Spawning work that panics
        let result = executor
            .spawn_sync(Box::new(|| {
                panic!("Intentional panic for testing");
                #[allow(unreachable_code)]
                Outcome::Events(vec![TestEvent::CpuWork(0)])
            }))
            .await;

        // Then: Panic is isolated and returns WorkerGone error
        assert!(result.is_err());
        match result.err().unwrap() {
            ExecutorError::WorkerGone => {} // Expected - panic causes worker to be gone
            other => panic!("Expected WorkerGone error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rayon_executor_maintains_performance_under_high_task_churn() {
        // Given: RayonSyncExecutor and high churn scenario
        let executor = RayonExecutor::<TestEvent>::new(Some(3));
        let iterations = 100usize;
        let start_time = Instant::now();

        // When: Rapidly spawning and completing many small tasks
        for iteration in 0..iterations {
            let result = executor
                .spawn_sync(Box::new(move || {
                    // Small amount of work per task
                    let value = iteration * 2;
                    #[allow(clippy::cast_possible_truncation)]
                    Outcome::Events(vec![TestEvent::CpuWork(value as u32)])
                }))
                .await;

            // Verify each task completes successfully
            assert!(result.is_ok(), "Task {iteration} failed");
        }

        let total_time = start_time.elapsed();

        // Then: High churn completes within reasonable time
        assert!(
            total_time < Duration::from_secs(2),
            "High task churn took too long: {total_time:?}"
        );
        println!(
            "Completed {} tasks in {:?} (avg: {:?}/task)",
            iterations,
            total_time,
            {
                #[allow(clippy::cast_possible_truncation)]
                {
                    total_time / iterations as u32
                }
            }
        );
    }

    #[tokio::test]
    async fn rayon_executor_handles_memory_intensive_sync_work_without_blocking_tokio() {
        // Given: RayonSyncExecutor and memory-intensive work
        let executor = RayonExecutor::<TestEvent>::new(Some(2));

        // When: Spawning memory-intensive synchronous work
        let memory_task = executor.spawn_sync(Box::new(|| {
            // Allocate and work with large data structure
            let mut large_vec = vec![0u8; 1024 * 1024]; // 1MB allocation

            // Perform CPU work on the data
            for (i, item) in large_vec.iter_mut().enumerate() {
                #[allow(clippy::cast_possible_truncation)]
                {
                    *item = (i % 256) as u8;
                }
            }

            let checksum: u32 = large_vec.iter().map(|&x| u32::from(x)).sum();
            Outcome::Events(vec![TestEvent::CpuWork(checksum)])
        }));

        // Simultaneously run async work to ensure Tokio isn't blocked
        let async_task = tokio::spawn(async {
            for i in 0..5 {
                tokio::time::sleep(Duration::from_millis(10)).await;
                if i == 4 {
                    return 42;
                }
            }
            0
        });

        // Then: Both tasks complete successfully without blocking each other
        let (sync_result, async_result) = tokio::join!(memory_task, async_task);

        assert!(sync_result.is_ok(), "Sync memory task failed");
        assert!(async_result.is_ok(), "Async task was blocked");
        assert_eq!(
            async_result.unwrap(),
            42,
            "Async task didn't complete properly"
        );
    }

    #[test]
    fn rayon_executor_shutdown_and_join_complete_immediately_for_sync_executor() {
        // Given: RayonSyncExecutor
        let executor = RayonExecutor::<TestEvent>::new(Some(2));

        // When: Calling shutdown and join
        let start = Instant::now();
        executor.shutdown(); // Should be immediate for Rayon

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            executor.join().await;
        });
        let shutdown_time = start.elapsed();

        // Then: Shutdown and join complete quickly (Rayon handles its own lifecycle)
        assert!(
            shutdown_time < Duration::from_millis(50),
            "Shutdown took too long: {shutdown_time:?}"
        );
    }

    #[tokio::test]
    async fn rayon_executor_handles_zero_thread_count_by_using_minimum_one_thread() {
        // Given: Request for zero threads (edge case)
        // When: Creating RayonSyncExecutor with 0 threads
        let executor = RayonExecutor::<TestEvent>::new(Some(0));

        // Then: Should still work (Rayon should use at least 1 thread)
        let result = executor
            .spawn_sync(Box::new(|| Outcome::Events(vec![TestEvent::CpuWork(123)])))
            .await;

        assert!(result.is_ok(), "Executor with 0 threads should still work");
    }
}
