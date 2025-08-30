//! TokioExecutor - Dedicated tokio runtime for effect handlers
//!
//! This executor owns (or attaches to) a tokio runtime that is separate from
//! any application event loop, ensuring effects do not contend with UI/event
//! processing. Shell distributes effects; the executor only spawns and manages
//! runtime services for them.

use crate::error::ShellError;
use crate::storage::{EmptyStorage, StorageBuilder};
use crate::task::{TaskId, TaskStats, TaskTracker};
use crate::timer::{Time, time};
use smallvec::SmallVec;
use std::future::Future;
use crossbeam_channel::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(feature = "tokio")]
use tokio::runtime;

/// TokioExecutor provides spawning and runtime services
///
/// This executor focuses purely on task spawning and runtime operations:
/// - Spawns tasks with safety guarantees (all cancelled on drop)
/// - Provides runtime services (timeout, etc.)
/// - Has its own resource storage
/// - Maintains task tracking for cleanup
/// - Shell handles effect distribution, not the executor
pub struct TokioExecutor<Event, Resources = EmptyStorage>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    /// Task tracker for managing spawned tasks
    task_tracker: Arc<Mutex<TaskTracker>>,
    
    /// Channel for sending events back to Core
    event_tx: Option<Sender<Event>>,
    
    /// Resources available to this executor's effect handlers
    resources: Resources,
    
    /// Runtime for timeout operations
    runtime: Time,
    
    /// Default timeout for tasks spawned by this executor
    default_timeout: Option<Duration>,

    /// Tokio runtime backing this executor (owned or provided handle)
    #[cfg(feature = "tokio")]
    runtime_mode: TokioRuntimeMode,
}

#[cfg(feature = "tokio")]
#[derive(Clone)]
enum TokioRuntimeMode {
    Owned(Arc<runtime::Runtime>),
    Handle(runtime::Handle),
}

impl<Event> TokioExecutor<Event, EmptyStorage>
where
    Event: Clone + Send + 'static,
{
    /// Create a new TokioExecutor with default configuration
    pub fn new() -> Self {
        #[cfg(feature = "tokio")]
        let rt = runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime");

        Self {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            event_tx: None,
            resources: EmptyStorage::new(),
            runtime: time(),
            default_timeout: Some(Duration::from_secs(30)),
            #[cfg(feature = "tokio")]
            runtime_mode: TokioRuntimeMode::Owned(Arc::new(rt)),
        }
    }

    /// Attach this executor to an existing tokio runtime handle
    #[cfg(feature = "tokio")]
    pub fn with_handle(handle: runtime::Handle) -> Self {
        Self {
            task_tracker: Arc::new(Mutex::new(TaskTracker::new())),
            event_tx: None,
            resources: EmptyStorage::new(),
            runtime: time(),
            default_timeout: Some(Duration::from_secs(30)),
            runtime_mode: TokioRuntimeMode::Handle(handle),
        }
    }
}

impl<Event> Default for TokioExecutor<Event, EmptyStorage>
where
    Event: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Event, Resources> TokioExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    // with_handle provided only for EmptyStorage version below

    /// Set the event sender for this executor
    pub fn with_event_sender(mut self, event_tx: Sender<Event>) -> Self {
        self.event_tx = Some(event_tx);
        self
    }
    
    /// Set the default timeout for tasks spawned by this executor
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.default_timeout = Some(timeout);
        self
    }
    
    /// Disable default timeout for tasks spawned by this executor
    pub fn without_timeout(mut self) -> Self {
        self.default_timeout = None;
        self
    }
    
    /// Add a resource to this executor
    pub fn with_resource<R: Send + Sync + 'static>(
        self,
        resource: R,
    ) -> TokioExecutor<Event, <Resources as StorageBuilder<R>>::Output>
    where
        Resources: StorageBuilder<R>,
        <Resources as StorageBuilder<R>>::Output: Clone + Send + Sync + 'static,
    {
        TokioExecutor {
            task_tracker: self.task_tracker.clone(),
            event_tx: self.event_tx.clone(),
            resources: self.resources.clone().with_model(resource),
            runtime: self.runtime,
            default_timeout: self.default_timeout,
            #[cfg(feature = "tokio")]
            runtime_mode: self.runtime_mode.clone(),
        }
    }
    
    /// Get task statistics for this executor
    pub fn task_stats(&self) -> TaskStats {
        if let Ok(tracker) = self.task_tracker.lock() {
            TaskStats {
                active_tasks: tracker.active_count(),
                is_closed: tracker.is_closed(),
            }
        } else {
            TaskStats {
                active_tasks: 0,
                is_closed: true,
            }
        }
    }
    
    
    /// Clean up finished tasks and return (cleaned_count, active_count)
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

    /// Spawn a tracked task that will be cleaned up on shutdown
    ///
    /// SAFETY GUARANTEE: All tasks spawned through this method will be cancelled
    /// when the TokioExecutor is dropped, preventing orphaned tasks.
    ///
    /// Performance: High-performance spawning using tokio runtime
    ///
    /// # Parameters
    /// - `future`: The async task to spawn
    ///
    /// # Returns
    /// - `Ok(TaskId)`: Task was successfully spawned and is being tracked
    /// - `Err(ShellError)`: Task tracker is closed or spawning failed
    pub fn spawn<F>(&self, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if let Ok(mut tracker) = self.task_tracker.lock() {
            #[cfg(feature = "tokio")]
            let handle: runtime::Handle = match &self.runtime_mode {
                TokioRuntimeMode::Owned(rt) => rt.handle().clone(),
                TokioRuntimeMode::Handle(h) => h.clone(),
            };
            #[cfg(feature = "tokio")]
            let spawner = move |f| { handle.spawn(f); };

            // Fallback when tokio feature is off (shouldn't be used)
            #[cfg(not(feature = "tokio"))]
            let spawner = |_f: F| {};

            let handle = tracker.spawn(
                spawner,
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

    /// Batch spawn for high-volume scenarios (>50 spawns)
    ///
    /// SAFETY GUARANTEE: All tasks in the batch will be cancelled when executor drops.
    /// Uses stack allocation with SmallVec for efficiency.
    ///
    /// # Parameters
    /// - `futures`: Iterator of futures to spawn
    ///
    /// # Returns
    /// - `Ok(Vec<TaskId>)`: All tasks were successfully spawned
    /// - `Err(ShellError)`: Task tracker is closed or spawning failed
    pub fn spawn_batch<I>(&self, futures: I) -> Result<Vec<TaskId>, ShellError>
    where
        I: IntoIterator,
        I::Item: Future<Output = ()> + Send + 'static,
    {
        let mut task_ids = SmallVec::<[TaskId; 64]>::new();
        for future in futures { task_ids.push(self.spawn(future)?); }
        Ok(task_ids.into_vec())
    }

    /// Spawn a task with timeout
    ///
    /// The task will be automatically cancelled if it doesn't complete within
    /// the specified duration.
    ///
    /// # Parameters
    /// - `duration`: Maximum time to wait for task completion
    /// - `future`: The async task to spawn
    ///
    /// # Returns
    /// - `Ok(TaskId)`: Task was successfully spawned with timeout
    /// - `Err(ShellError)`: Task tracker is closed or spawning failed
    pub fn spawn_with_timeout<F>(&self, duration: Duration, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let runtime = self.runtime;
        let timeout_future = async move {
            let timeout_result = runtime.timeout(duration, Box::pin(future)).await;
            if timeout_result.is_err() {
                // Task timed out - could send timeout event here if needed
            }
        };

        self.spawn(timeout_future)
    }

    /// Get the runtime implementation used by this executor
    #[must_use]
    pub fn runtime(&self) -> Time {
        self.runtime
    }

}

// Clone implementation - shares the same TaskTracker for cleanup
impl<Event, Resources> Clone for TokioExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            task_tracker: Arc::clone(&self.task_tracker), // Share task tracker for cleanup
            event_tx: self.event_tx.clone(),
            resources: self.resources.clone(),
            runtime: self.runtime,
            default_timeout: self.default_timeout,
            #[cfg(feature = "tokio")]
            runtime_mode: self.runtime_mode.clone(),
        }
    }
}

// Explicit Drop implementation for guaranteed task cleanup
impl<Event, Resources> Drop for TokioExecutor<Event, Resources> 
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn drop(&mut self) {
        // Force cancel all tasks immediately - no graceful waiting
        // This ensures no orphaned tasks when executor is dropped
        if let Ok(mut tracker) = self.task_tracker.lock() {
            #[cfg(feature = "tracing")]
            {
                let active = tracker.active_count();
                if active > 0 {
                    tracing::warn!("TokioExecutor dropped with {} active tasks - force aborting all", active);
                }
            }
            
            // Force abort all active tasks - no mercy, no orphans
            tracker.abort_all();
        }
        
        #[cfg(not(feature = "tracing"))]
        {
            // In release builds without tracing, silently clean up
            // The TaskTracker Drop implementation will handle actual task cancellation
        }
    }
}

// Implement the new SpawnExecutor trait focused on spawning and runtime services
impl<Event, Resources> crate::executor::SpawnExecutor<Event, Resources> for TokioExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    fn spawn<F>(&self, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn(future)  // Delegate to our own spawn method
    }

    fn spawn_batch<I>(&self, futures: I) -> Result<Vec<TaskId>, ShellError>
    where
        I: IntoIterator,
        I::Item: Future<Output = ()> + Send + 'static,
    {
        self.spawn_batch(futures)  // Delegate to our own spawn_batch method
    }

    fn spawn_with_timeout<F>(&self, duration: Duration, future: F) -> Result<TaskId, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.spawn_with_timeout(duration, future)  // Delegate to our own method
    }

    fn runtime(&self) -> crate::timer::Time {
        self.runtime()  // Delegate to our own runtime method
    }

    fn resource<T, Index>(&self) -> &T
    where
        Resources: crate::storage::Selector<T, Index>,
    {
        self.resources.get()
    }

    fn task_stats(&self) -> crate::task::TaskStats {
        self.task_stats()  // Delegate to our own method
    }

    fn cleanup_finished_tasks(&self) -> (usize, usize) {
        self.cleanup_finished_tasks()  // Delegate to our own method
    }
}

// Provide access to the executor's resource storage for magic handler helpers
impl<Event, Resources> crate::executor::HasExecutorResources<Event>
    for TokioExecutor<Event, Resources>
where
    Event: Clone + Send + 'static,
    Resources: Clone + Send + Sync + 'static,
{
    type Resources = Resources;

    fn clone_resources(&self) -> Self::Resources {
        self.resources.clone()
    }
}
