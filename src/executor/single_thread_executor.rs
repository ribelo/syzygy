//! SingleThreadExecutor - Sequential, single-threaded executor for serialized tasks
//!
//! This executor runs all submitted futures on a dedicated OS thread, one after another,
//! guaranteeing strict FIFO ordering. It's ideal for single-writer patterns like
//! database writes where concurrent access must be avoided.

use crate::error::ShellError;
use crate::task::{TaskId, TaskStats, TaskTracker};
use crate::timer::{Time, time};
use crossbeam_channel::{unbounded, Sender};
use futures_util::future::BoxFuture;
use std::future::Future;
use std::sync::{Arc, Mutex};

/// Internal work queue type
type Job = BoxFuture<'static, ()>;

/// SingleThreadExecutor provides FIFO, single-threaded task execution
pub struct SingleThreadExecutor<Event, Resources = EmptyStorage>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    task_tracker: Arc<Mutex<TaskTracker>>,
    event_tx: Option<Sender<Event>>,
    resources: Resources,
    runtime: Time,
    job_tx: Sender<Job>,
}

impl<Event> SingleThreadExecutor<Event, EmptyStorage>
where
    Event: Clone + Send + 'static,
{
    /// Create a new SingleThreadExecutor with default configuration
    pub fn new() -> Self {
        let (job_tx, job_rx) = unbounded::<Job>();

        // Spawn dedicated worker thread that runs jobs sequentially
        std::thread::spawn(move || {
            while let Ok(job) = job_rx.recv() {
                // Run the future to completion sequentially
                futures::executor::block_on(job);
            }
        });

        Self {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            event_tx: None,
            resources: Default::default(),
            runtime: time(),
            job_tx,
        }
    }
}

impl<Event> Default for SingleThreadExecutor<Event, EmptyStorage>
where
    Event: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Event, Resources> SingleThreadExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    /// Set the event sender for this executor
    pub fn with_event_sender(mut self, event_tx: Sender<Event>) -> Self {
        self.event_tx = Some(event_tx);
        self
    }

    /// Add a resource to this executor
    pub fn with_resource<R: Send + Sync + 'static>(
        self,
        resource: R,
    ) -> SingleThreadExecutor<Event, <Resources as StorageBuilder<R>>::Output>
    where
        Resources: StorageBuilder<R>,
        <Resources as StorageBuilder<R>>::Output: Clone + Send + Sync + 'static,
    {
        SingleThreadExecutor {
            task_tracker: self.task_tracker.clone(),
            event_tx: self.event_tx.clone(),
            resources: self.resources.clone().with_model(resource),
            runtime: self.runtime,
            job_tx: self.job_tx.clone(),
        }
    }

    /// Get task statistics
    pub fn task_stats(&self) -> TaskStats {
        if let Ok(tracker) = self.task_tracker.lock() {
            TaskStats {
                active_tasks: tracker.active_count(),
                is_closed: tracker.is_closed(),
            }
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

impl<Event, Resources> Clone for SingleThreadExecutor<Event, Resources>
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
            job_tx: self.job_tx.clone(),
        }
    }
}

// Implement SpawnExecutor over a single-thread worker
impl<Event, Resources> crate::executor::Executor<Event, Resources> for SingleThreadExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn spawn<F>(&self, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Use TaskTracker to register the task and enqueue the job
        if let Ok(mut tracker) = self.task_tracker.lock() {
            let handle = tracker.spawn(
                |fut| {
                    // Enqueue the future into the single-thread queue
                    // Wrap in a BoxFuture to unify type at the queue boundary
                    let _ = self.job_tx.send(Box::pin(fut));
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
        for fut in futures {
            ids.push(self.spawn(fut)?);
        }
        Ok(ids)
    }

    fn spawn_with_timeout<F>(&self, _duration: std::time::Duration, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Best-effort: single-threaded executor does not enforce timeouts.
        // Users should implement cooperative cancellation within the future.
        self.spawn(future)
    }

    fn runtime(&self) -> Time {
        self.runtime
    }

    fn resource<T, Index>(&self) -> &T
    where
        Resources: crate::storage::Selector<T, Index>,
    {
        self.resources.get()
    }

    fn task_stats(&self) -> TaskStats {
        self.task_stats()
    }

    fn cleanup_finished_tasks(&self) -> (usize, usize) {
        self.cleanup_finished_tasks()
    }
}

// Provide access to the executor's resource storage for magic handler helpers
impl<Event, Resources> crate::executor::HasExecutorResources<Event>
    for SingleThreadExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    type Resources = Resources;

    fn clone_resources(&self) -> Self::Resources {
        self.resources.clone()
    }
}
