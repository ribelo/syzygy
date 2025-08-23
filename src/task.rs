use std::future::Future;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use futures_util::future::BoxFuture;
use crate::error::ShellError;
use crate::spawn::Spawn;

/// Unique identifier for spawned tasks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(u64);

impl Default for TaskId {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskId {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        Self(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

/// Task handle for monitoring and controlling spawned tasks
pub struct TaskHandle {
    id: TaskId,
    is_finished: Arc<AtomicBool>,
}

impl TaskHandle {
    fn new(id: TaskId) -> (Self, Arc<AtomicBool>) {
        let is_finished = Arc::new(AtomicBool::new(false));
        let handle = Self {
            id,
            is_finished: Arc::clone(&is_finished),
        };
        (handle, is_finished)
    }

    #[must_use] pub fn id(&self) -> TaskId {
        self.id
    }

    #[must_use] pub fn is_finished(&self) -> bool {
        self.is_finished.load(Ordering::Relaxed)
    }
}

impl Clone for TaskHandle {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            is_finished: Arc::clone(&self.is_finished),
        }
    }
}

/// Runtime-agnostic task tracker for managing async tasks
///
/// This works with any async runtime (tokio, smol, async-std) by accepting
/// spawn functions that match the runtime's interface.
pub struct TaskTracker {
    tasks: HashMap<TaskId, TaskHandle>,
    active_count: AtomicUsize,
    is_closed: Arc<AtomicBool>,
}

impl TaskTracker {
    #[must_use] pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            active_count: AtomicUsize::new(0),
            is_closed: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Spawn a task with zero-cost spawning using the unified spawner (no boxing)
    ///
    /// This uses zero-cost spawn functions from Rust 1.85+ AsyncFn support.
    /// Uses `spawner()` internally to eliminate boxing overhead.
    pub fn spawn_direct<F>(&mut self, future: F) -> Result<TaskHandle, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        use crate::spawn::spawner;

        if self.is_closed.load(Ordering::Relaxed) {
            return Err(ShellError::TaskTrackerClosed);
        }

        let task_id = TaskId::new();
        let (handle, is_finished) = TaskHandle::new(task_id);

        // Wrap the future with completion notification - no boxing!
        let wrapped_future = async move {
            future.await;
            is_finished.store(true, Ordering::Relaxed);
        };

        // Spawn using the unified spawner - no boxing!
        spawner().spawn(wrapped_future);

        self.tasks.insert(task_id, handle);

        // Increment active count when task is spawned
        self.active_count.fetch_add(1, Ordering::Relaxed);

        Ok(self.tasks[&task_id].clone())
    }

    /// Spawn a task using the provided spawn function (legacy boxed API)
    ///
    /// DEPRECATED: Use spawn_direct for zero-cost spawning.
    /// The spawn_fn should match your runtime's spawn function signature
    pub fn spawn<F, Fut, SpawnFn>(&mut self, spawn_fn: SpawnFn, future: Fut) -> Result<TaskHandle, ShellError>
    where
        F: Future<Output = ()> + Send + 'static,
        Fut: FnOnce(Arc<AtomicBool>) -> F,
        SpawnFn: Fn(F),
    {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(ShellError::TaskTrackerClosed);
        }

        let task_id = TaskId::new();
        let (handle, is_finished) = TaskHandle::new(task_id);

        // Create the future with finish notification
        let task_future = future(is_finished);

        // Spawn using the provided function
        spawn_fn(task_future);

        self.tasks.insert(task_id, handle);

        // Increment active count when task is spawned
        self.active_count.fetch_add(1, Ordering::Relaxed);

        Ok(self.tasks[&task_id].clone())
    }

    /// Spawn multiple tasks in batch with zero-cost spawning (no boxing)
    ///
    /// NOTE: This is a placeholder for future zero-cost batch implementation.
    /// The type system complexity makes this challenging, so for Phase 1 we'll
    /// focus on individual zero-cost spawning which provides most of the benefit.
    pub fn spawn_batch_direct_placeholder(&mut self) {
        // TODO: Implement zero-cost batch spawning
        // For now, use multiple spawn_direct calls for zero-cost spawning
    }

    /// Spawn multiple tasks in batch with single mutex lock (legacy boxed API)
    ///
    /// DEPRECATED: Use spawn_batch_direct for zero-cost batch spawning.
    /// This is more efficient than multiple individual spawn calls as it only
    /// requires one TaskTracker lock for the entire batch.
    pub fn spawn_batch<SpawnFn>(
        &mut self,
        spawn_fn: &SpawnFn,
        futures: Vec<(TaskId, BoxFuture<'static, ()>)>
    ) -> Result<Vec<TaskHandle>, ShellError>
    where
        SpawnFn: Fn(BoxFuture<'static, ()>) + ?Sized,
    {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(ShellError::TaskTrackerClosed);
        }

        if futures.is_empty() {
            return Ok(Vec::new());
        }

        let mut handles = Vec::with_capacity(futures.len());

        // Process each future in the batch
        for (task_id, boxed_future) in futures {
            let (handle, is_finished) = TaskHandle::new(task_id);

            // Wrap the future with completion notification
            let wrapped_future = Box::pin(async move {
                boxed_future.await;
                is_finished.store(true, Ordering::Relaxed);
            });

            // Spawn using the provided function
            spawn_fn(wrapped_future);

            // Track the task
            self.tasks.insert(task_id, handle.clone());
            handles.push(handle);
        }

        // Single atomic increment for the entire batch
        self.active_count.fetch_add(handles.len(), Ordering::Relaxed);

        Ok(handles)
    }

    /// Get the number of active tasks
    ///
    /// O(1) operation using cached counter instead of O(n) scan.
    pub fn active_count(&self) -> usize {
        self.active_count.load(Ordering::Relaxed)
    }

    /// Clean up finished tasks
    pub fn cleanup_finished(&mut self) {
        self.tasks.retain(|_, handle| {
            if handle.is_finished() {
                // Decrement counter when removing finished task
                self.active_count.fetch_sub(1, Ordering::Relaxed);
                false
            } else {
                true
            }
        });
    }

    /// Close the tracker - no new tasks can be spawned
    pub fn close(&self) {
        self.is_closed.store(true, Ordering::Relaxed);
    }

    /// Check if the tracker is closed
    pub fn is_closed(&self) -> bool {
        self.is_closed.load(Ordering::Relaxed)
    }
}

impl Default for TaskTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the task tracker
#[derive(Debug, Clone)]
pub struct TaskStats {
    pub active_tasks: usize,
    pub is_closed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_tracker() {
        let mut tracker = TaskTracker::new();

        // Mock spawn function that does nothing
        let spawn_fn = |_future| {};

        let handle = tracker.spawn(spawn_fn, |is_finished| async move {
            // Simulate task completion
            is_finished.store(true, Ordering::Relaxed);
        }).unwrap();

        assert_eq!(tracker.active_count(), 1);

        // Mark task as finished and cleanup
        handle.is_finished.store(true, Ordering::Relaxed);
        tracker.cleanup_finished();

        assert_eq!(tracker.active_count(), 0);
    }

    #[test]
    fn test_task_tracker_close() {
        let mut tracker = TaskTracker::new();

        tracker.close();
        assert!(tracker.is_closed());

        let spawn_fn = |_future| {};
        let result = tracker.spawn(spawn_fn, |_| async {});

        assert!(matches!(result, Err(ShellError::TaskTrackerClosed)));
    }
}
