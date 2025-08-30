//! RayonExecutor — high-throughput CPU-bound executor using Rayon
//!
//! This executor submits CPU-heavy futures to a dedicated Rayon thread pool.
//! Each future is driven to completion by blocking on it inside a Rayon worker,
//! so handlers routed here should be pure compute (no tokio APIs).

use crate::error::ShellError;
use crate::storage::{EmptyStorage, StorageBuilder};
use crate::task::{TaskId, TaskStats, TaskTracker};
use crate::timer::{Time, time};
use futures::executor::block_on;
use rayon::ThreadPool;
use rayon::ThreadPoolBuilder;
use std::future::Future;
use std::sync::{Arc, Mutex};

/// Rayon-based CPU-bound executor
pub struct RayonExecutor<Event, Resources = EmptyStorage>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    task_tracker: Arc<Mutex<TaskTracker>>,
    event_tx: Option<crossbeam_channel::Sender<Event>>,
    resources: Resources,
    runtime: Time,
    pool: Arc<ThreadPool>,
}

impl<Event> RayonExecutor<Event, EmptyStorage>
where
    Event: Clone + Send + 'static,
{
    /// Create a new RayonExecutor with a thread pool sized to available CPUs
    pub fn new() -> Self {
        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .max(1);

        let pool = ThreadPoolBuilder::new()
            .num_threads(n_threads)
            .thread_name(|i| format!("syzygy-rayon-{i}"))
            .build()
            .expect("failed to build rayon thread pool");

        Self {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            event_tx: None,
            resources: EmptyStorage::new(),
            runtime: time(),
            pool: Arc::new(pool),
        }
    }
}

impl<Event> Default for RayonExecutor<Event, EmptyStorage>
where
    Event: Clone + Send + 'static,
{
    fn default() -> Self { Self::new() }
}

impl<Event, Resources> RayonExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    /// Set the event sender for this executor
    pub fn with_event_sender(mut self, event_tx: crossbeam_channel::Sender<Event>) -> Self {
        self.event_tx = Some(event_tx);
        self
    }

    /// Add a resource to this executor
    pub fn with_resource<R: Send + Sync + 'static>(
        self,
        resource: R,
    ) -> RayonExecutor<Event, <Resources as StorageBuilder<R>>::Output>
    where
        Resources: StorageBuilder<R>,
        <Resources as StorageBuilder<R>>::Output: Clone + Send + Sync + 'static,
    {
        RayonExecutor {
            task_tracker: self.task_tracker.clone(),
            event_tx: self.event_tx.clone(),
            resources: self.resources.clone().with_model(resource),
            runtime: self.runtime,
            pool: self.pool.clone(),
        }
    }

    /// Get task statistics
    pub fn task_stats(&self) -> TaskStats {
        if let Ok(tracker) = self.task_tracker.lock() {
            TaskStats { active_tasks: tracker.active_count(), is_closed: tracker.is_closed() }
        } else {
            TaskStats { active_tasks: 0, is_closed: true }
        }
    }

    /// Cleanup finished tasks and return (cleaned_count, active_count)
    pub fn cleanup_finished_tasks(&self) -> (usize, usize) {
        if let Ok(mut tracker) = self.task_tracker.lock() {
            let cleaned = tracker.active_count();
            tracker.cleanup_finished();
            let active = tracker.active_count();
            (cleaned, active)
        } else {
            (0, 0)
        }
    }
}

impl<Event, Resources> Clone for RayonExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            task_tracker: Arc::clone(&self.task_tracker),
            event_tx: self.event_tx.clone(),
            resources: self.resources.clone(),
            runtime: self.runtime,
            pool: Arc::clone(&self.pool),
        }
    }
}

impl<Event, Resources> crate::executor::SpawnExecutor<Event, Resources> for RayonExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn spawn<F>(&self, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Ok(mut tracker) = self.task_tracker.lock() {
            let handle = tracker.spawn(
                {
                    let pool = self.pool.clone();
                    move |fut| {
                        pool.spawn(move || {
                            block_on(fut);
                        });
                    }
                },
                move |is_finished| async move {
                    future.await;
                    is_finished.store(true, std::sync::atomic::Ordering::Relaxed);
                },
            )?;
            Ok(handle.id())
        } else {
            Err(ShellError::TaskTrackerClosed)
        }
    }

    fn spawn_batch<I>(&self, futures: I) -> Result<Vec<TaskId>, ShellError>
    where
        I: IntoIterator,
        I::Item: Future<Output = ()> + Send + 'static,
    {
        let mut ids = Vec::new();
        for fut in futures { ids.push(self.spawn(fut)?); }
        Ok(ids)
    }

    fn spawn_with_timeout<F>(&self, duration: std::time::Duration, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let rt = self.runtime;
        self.spawn(async move {
            let _ = rt.timeout(duration, Box::pin(future)).await;
        })
    }

    fn runtime(&self) -> Time { self.runtime }

    fn resource<T, Index>(&self) -> &T
    where
        Resources: crate::storage::Selector<T, Index>,
    {
        self.resources.get()
    }

    fn task_stats(&self) -> TaskStats { self.task_stats() }

    fn cleanup_finished_tasks(&self) -> (usize, usize) { self.cleanup_finished_tasks() }
}

impl<Event, Resources> crate::executor::HasExecutorResources<Event> for RayonExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    type Resources = Resources;
    fn clone_resources(&self) -> Self::Resources { self.resources.clone() }
}

