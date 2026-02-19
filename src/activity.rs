//! # Activity Tracker
//!
//! This module provides the `Activity` struct, which tracks in-flight async work
//! beyond the effect queue. This is essential for proper "await idle" functionality
//! in CLI applications where we need to know when all async work has completed.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug)]
struct ActivityInner {
    inflight: AtomicUsize,
    mutex: Mutex<()>,
    condvar: Condvar,
}

/// Tracks in-flight async work beyond the effect queue
///
/// This is used to determine when the system is truly idle, accounting for
/// async jobs that have been spawned but may not have completed yet.
#[derive(Debug, Clone)]
pub struct Activity {
    inner: Arc<ActivityInner>,
}

impl Activity {
    /// Create a new Activity tracker
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(ActivityInner {
                inflight: AtomicUsize::new(0),
                mutex: Mutex::new(()),
                condvar: Condvar::new(),
            }),
        }
    }

    /// Increment the in-flight counter
    ///
    /// Call this when spawning async work.
    pub fn inc(&self) {
        self.inner.inflight.fetch_add(1, Ordering::AcqRel);
    }

    /// Decrement the in-flight counter
    ///
    /// Call this when async work completes. This will notify any waiters
    /// if the count reaches zero.
    pub fn dec(&self) {
        let prev = self.inner.inflight.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(prev > 0, "activity counter underflow");
        if prev == 1 {
            // We just reached zero, notify waiters
            let guard = self.inner.mutex.lock().unwrap();
            // Hold the lock briefly to pair with wait_until_zero
            self.inner.condvar.notify_all();
            drop(guard);
        }
    }

    /// Get the current count of in-flight jobs
    #[must_use]
    pub fn load(&self) -> usize {
        self.inner.inflight.load(Ordering::Acquire)
    }

    /// Wait until the in-flight count reaches zero or timeout expires
    ///
    /// Returns true if count reached zero, false if timeout occurred.
    #[must_use]
    pub fn wait_until_zero(&self, timeout: Duration) -> bool {
        if self.load() == 0 {
            return true;
        }

        if timeout.is_zero() {
            return self.load() == 0;
        }

        let deadline = if timeout == Duration::MAX {
            None
        } else {
            Instant::now().checked_add(timeout)
        };

        let mut guard = self.inner.mutex.lock().unwrap();
        loop {
            if self.load() == 0 {
                return true;
            }

            match deadline {
                Some(deadline) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return false;
                    }

                    let wait_duration = deadline
                        .checked_duration_since(now)
                        .unwrap_or(Duration::ZERO);

                    let result = self
                        .inner
                        .condvar
                        .wait_timeout(guard, wait_duration)
                        .unwrap();
                    guard = result.0;
                    if result.1.timed_out() && self.load() != 0 {
                        return false;
                    }
                }
                None => {
                    guard = self.inner.condvar.wait(guard).unwrap();
                }
            }
        }
    }
}

impl Default for Activity {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_activity_basic() {
        let activity = Activity::new();
        assert_eq!(activity.load(), 0);

        activity.inc();
        assert_eq!(activity.load(), 1);

        activity.dec();
        assert_eq!(activity.load(), 0);
    }

    #[test]
    fn test_activity_wait_until_zero_immediate() {
        let activity = Activity::new();
        assert!(activity.wait_until_zero(Duration::from_millis(100)));
    }

    #[test]
    fn test_activity_wait_until_zero_timeout() {
        let activity = Activity::new();
        activity.inc();

        let start = std::time::Instant::now();
        let result = activity.wait_until_zero(Duration::from_millis(10));
        let elapsed = start.elapsed();

        assert!(!result);
        assert!(elapsed >= Duration::from_millis(10));
    }

    #[test]
    fn test_activity_wait_until_zero_with_completion() {
        let activity = Arc::new(Activity::new());
        activity.inc();

        let activity_clone = Arc::clone(&activity);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            activity_clone.dec();
        });

        let start = std::time::Instant::now();
        let result = activity.wait_until_zero(Duration::from_millis(200));
        let elapsed = start.elapsed();

        assert!(result);
        assert!(elapsed >= Duration::from_millis(40)); // Allow some tolerance
        assert!(elapsed < Duration::from_millis(150));
    }

    #[test]
    fn test_activity_clone() {
        let activity1 = Activity::new();
        activity1.inc();

        let activity2 = activity1.clone();
        assert_eq!(activity2.load(), 1);

        activity2.dec();
        assert_eq!(activity1.load(), 0);
        assert_eq!(activity2.load(), 0);
    }
}
