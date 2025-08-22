//! Task Collector for batch task registration
//! 
//! This implements the performance engineer's recommended Option 5:
//! Per-Effect Task Collector that accumulates spawn requests and
//! batch-registers them with TaskTracker to avoid mutex contention.

use smallvec::SmallVec;
use futures_util::future::BoxFuture;
use std::future::Future;
use crate::task::TaskId;

/// Lightweight collector that accumulates spawn requests during effect execution
/// 
/// Uses SmallVec to stack-allocate for common case of <=4 tasks per effect.
/// This avoids heap allocation in the majority of use cases while still
/// supporting effects that spawn many tasks.
pub struct TaskCollector {
    /// Pending spawns to be batch-registered
    /// SmallVec<[T; 4]> stack-allocates for <=4 items, heap for more
    pending_spawns: SmallVec<[(TaskId, BoxFuture<'static, ()>); 4]>,
}

impl TaskCollector {
    /// Create a new task collector
    #[must_use] pub fn new() -> Self {
        Self {
            pending_spawns: SmallVec::new(),
        }
    }
    
    /// Add a task to be spawned later
    /// 
    /// This is lock-free and very fast - just pushes to SmallVec.
    /// The actual spawning happens during batch registration.
    pub fn collect<F>(&mut self, future: F) -> TaskId
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let task_id = TaskId::new();
        self.pending_spawns.push((task_id, Box::pin(future)));
        task_id
    }
    
    /// Get the pending spawns for batch processing
    /// 
    /// This drains the internal collection and returns the tasks
    /// to be registered with TaskTracker.
    pub fn drain_pending(&mut self) -> impl Iterator<Item = (TaskId, BoxFuture<'static, ()>)> + '_ {
        self.pending_spawns.drain(..)
    }
    
    /// Check if there are pending spawns
    #[must_use] pub fn has_pending(&self) -> bool {
        !self.pending_spawns.is_empty()
    }
    
    /// Get the number of pending spawns
    #[must_use] pub fn pending_count(&self) -> usize {
        self.pending_spawns.len()
    }
    
    /// Clear all pending spawns (for error recovery)
    pub fn clear(&mut self) {
        self.pending_spawns.clear();
    }
}

impl Default for TaskCollector {
    fn default() -> Self {
        Self::new()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    
    #[test]
    fn test_task_collector_basic() {
        let mut collector = TaskCollector::new();
        
        assert!(!collector.has_pending());
        assert_eq!(collector.pending_count(), 0);
        
        // Collect some tasks
        let _id1 = collector.collect(async { });
        let _id2 = collector.collect(async { });
        
        assert!(collector.has_pending());
        assert_eq!(collector.pending_count(), 2);
        
        // Drain tasks
        let tasks: Vec<_> = collector.drain_pending().collect();
        assert_eq!(tasks.len(), 2);
        
        assert!(!collector.has_pending());
        assert_eq!(collector.pending_count(), 0);
    }
    
    #[test]
    fn test_task_collector_many_tasks() {
        let mut collector = TaskCollector::new();
        
        // Test that it works with >4 tasks (forces heap allocation)
        for i in 0..10 {
            let _id = collector.collect(async move {
                tokio::time::sleep(Duration::from_millis(i)).await;
            });
        }
        
        assert_eq!(collector.pending_count(), 10);
        
        let tasks: Vec<_> = collector.drain_pending().collect();
        assert_eq!(tasks.len(), 10);
    }
    
    #[test]
    fn test_task_collector_clear() {
        let mut collector = TaskCollector::new();
        
        let _id1 = collector.collect(async { });
        let _id2 = collector.collect(async { });
        
        assert_eq!(collector.pending_count(), 2);
        
        collector.clear();
        
        assert_eq!(collector.pending_count(), 0);
        assert!(!collector.has_pending());
    }
}