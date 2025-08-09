//! Task tracking for structured concurrency

use std::sync::{Arc, Mutex};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// Tracks spawned tasks and provides structured concurrency
#[derive(Debug, Clone)]
pub struct TaskTracker {
    tasks: Arc<Mutex<Vec<JoinHandle<()>>>>,
    shutdown: CancellationToken,
}

impl Default for TaskTracker {
    fn default() -> Self {
        Self {
            tasks: Arc::new(Mutex::new(Vec::new())),
            shutdown: CancellationToken::new(),
        }
    }
}

impl TaskTracker {
    /// Create a new task tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn a tracked task
    pub fn spawn<F>(&self, future: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let shutdown = self.shutdown.clone();
        
        // Use tokio::spawn directly - it will work if we're in runtime context
        let handle = tokio::spawn(async move {
            tokio::select! {
                _ = shutdown.cancelled() => {
                    // Task cancelled by shutdown
                }
                _ = future => {
                    // Task completed normally
                }
            }
        });
        
        self.tasks.lock().unwrap().push(handle);
    }

    /// Signal shutdown to all tasks
    pub fn shutdown(&self) {
        self.shutdown.cancel();
    }

    /// Wait for all tasks to complete or timeout
    pub async fn wait_for_shutdown(&self, timeout: std::time::Duration) {
        self.shutdown();
        
        let tasks = {
            let mut guard = self.tasks.lock().unwrap();
            std::mem::take(&mut *guard)
        };

        // Wait for all tasks with timeout
        let _ = tokio::time::timeout(timeout, async {
            for task in tasks {
                let _ = task.await;
            }
        }).await;
    }

    /// Abort all tasks immediately
    pub fn abort_all(&self) {
        let mut tasks = self.tasks.lock().unwrap();
        for task in tasks.drain(..) {
            task.abort();
        }
    }
}

impl Drop for TaskTracker {
    fn drop(&mut self) {
        // Cancel all tasks
        self.shutdown.cancel();
        
        // Abort any remaining tasks
        self.abort_all();
    }
}