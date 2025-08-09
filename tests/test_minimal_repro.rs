#[cfg(feature = "async")]
#[tokio::test]
async fn test_exact_task_tracker_pattern() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::time::sleep;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;
    
    struct MinimalTracker {
        shutdown: CancellationToken,
    }
    
    impl MinimalTracker {
        fn spawn<F>(&self, future: F)
        where
            F: std::future::Future<Output = ()> + Send + 'static,
        {
            println!("MinimalTracker::spawn called");
            let shutdown = self.shutdown.clone();
            
            let runtime_handle = tokio::runtime::Handle::try_current()
                .expect("No runtime");
            
            println!("Got handle, spawning...");
            runtime_handle.spawn(async move {
                println!("Inside spawned task!");
                tokio::select! {
                    _ = shutdown.cancelled() => {
                        println!("Cancelled");
                    }
                    _ = future => {
                        println!("Future completed");
                    }
                }
            });
            println!("Spawn done");
        }
    }
    
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();
    
    let tracker = MinimalTracker {
        shutdown: CancellationToken::new(),
    };
    
    // Call spawn from sync context (simulating handle_effects)
    fn call_spawn_sync(tracker: &MinimalTracker, flag: Arc<AtomicBool>) {
        println!("In sync context");
        tracker.spawn(async move {
            println!("User task running!");
            flag.store(true, Ordering::SeqCst);
        });
        println!("Sync context done");
    }
    
    call_spawn_sync(&tracker, flag_clone);
    
    println!("Back in async, yielding...");
    tokio::task::yield_now().await;
    sleep(Duration::from_millis(100)).await;
    
    println!("Flag: {}", flag.load(Ordering::SeqCst));
    assert!(flag.load(Ordering::SeqCst), "Task should have run");
}