//! ThreadPerCoreTokioExecutor — actix-like per-core tokio runtimes
//!
//! Spawns one single-threaded tokio runtime per CPU core and load-balances
//! submitted futures across them in round-robin order. Futures execute on the
//! chosen core's runtime with minimal contention.

#[cfg(feature = "tokio")]
use tokio::runtime;

use crate::error::ShellError;
use crate::task::{TaskId, TaskStats, TaskTracker};
use crate::timer::{Time, time};

#[cfg(feature = "tokio")]
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use std::future::Future;
use futures_util::future::BoxFuture;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};

type Job = BoxFuture<'static, ()>;

/// Per-core tokio executors with round-robin dispatch
pub struct ThreadPerCoreTokioExecutor<Event, Resources = EmptyStorage>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    task_tracker: Arc<Mutex<TaskTracker>>,
    event_tx: Option<crossbeam_channel::Sender<Event>>,
    resources: Resources,
    runtime: Time,
    senders: Arc<Vec<UnboundedSender<Job>>>,
    rr: AtomicUsize,
}

impl<Event> ThreadPerCoreTokioExecutor<Event, EmptyStorage>
where
    Event: Clone + Send + 'static,
{
    /// Create a new thread-per-core executor
    pub fn new() -> Self {
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .max(1);

        let mut senders = Vec::with_capacity(cores);

        for _ in 0..cores {
            // Channel for jobs to this core
            #[cfg(feature = "tokio")]
            let (tx, mut rx) = unbounded_channel::<Job>();

            // Dedicated OS thread with a current-thread tokio runtime
            std::thread::spawn(move || {
                #[cfg(feature = "tokio")]
                let rt = runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build tokio runtime");

                #[cfg(feature = "tokio")]
                rt.block_on(async move {
                    while let Some(job) = rx.recv().await {
                        // Spawn onto this runtime (Send + 'static assumed)
                        tokio::spawn(job);
                    }
                });
            });

            senders.push(tx);
        }

        Self {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            event_tx: None,
            resources: Default::default(),
            runtime: time(),
            senders: Arc::new(senders),
            rr: AtomicUsize::new(0),
        }
    }
}

impl<Event> Default for ThreadPerCoreTokioExecutor<Event, EmptyStorage>
where
    Event: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Event, Resources> ThreadPerCoreTokioExecutor<Event, Resources>
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
    ) -> ThreadPerCoreTokioExecutor<Event, <Resources as StorageBuilder<R>>::Output>
    where
        Resources: StorageBuilder<R>,
        <Resources as StorageBuilder<R>>::Output: Clone + Send + Sync + 'static,
    {
        ThreadPerCoreTokioExecutor {
            task_tracker: self.task_tracker.clone(),
            event_tx: self.event_tx.clone(),
            resources: self.resources.clone().with_model(resource),
            runtime: self.runtime,
            senders: self.senders.clone(),
            rr: AtomicUsize::new(self.rr.load(Ordering::Relaxed)),
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

    fn next_sender(&self) -> &UnboundedSender<Job> {
        let idx = self.rr.fetch_add(1, Ordering::Relaxed) % self.senders.len();
        &self.senders[idx]
    }
}

impl<Event, Resources> Clone for ThreadPerCoreTokioExecutor<Event, Resources>
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
            senders: Arc::clone(&self.senders),
            rr: AtomicUsize::new(self.rr.load(Ordering::Relaxed)),
        }
    }
}

impl<Event, Resources> crate::executor::Executor<Event, Resources>
    for ThreadPerCoreTokioExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn spawn<F>(&self, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let sender = self.next_sender().clone();

        if let Ok(mut tracker) = self.task_tracker.lock() {
            let handle = tracker.spawn(
                move |fut| {
                    let _ = sender.send(Box::pin(fut));
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

    fn spawn_with_timeout<F>(&self, duration: std::time::Duration, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        // Wrap with timeout using runtime abstraction
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

impl<Event, Resources> crate::executor::HasExecutorResources<Event>
    for ThreadPerCoreTokioExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    type Resources = Resources;
    fn clone_resources(&self) -> Self::Resources { self.resources.clone() }
}
