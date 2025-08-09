#[cfg(feature = "async")]
#[tokio::test]
async fn test_direct_tokio_spawn() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::time::sleep;
    use std::time::Duration;
    
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();
    
    println!("Before spawn");
    
    // Direct tokio spawn - no fancy shit
    let handle = tokio::spawn(async move {
        println!("Task running!");
        flag_clone.store(true, Ordering::SeqCst);
    });
    
    println!("After spawn, waiting...");
    sleep(Duration::from_millis(100)).await;
    
    println!("Flag: {}", flag.load(Ordering::SeqCst));
    assert!(flag.load(Ordering::SeqCst), "Direct spawn should work");
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_spawn_with_handle() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::time::sleep;
    use std::time::Duration;
    
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();
    
    println!("Before spawn with handle");
    
    // Spawn using runtime handle
    let runtime_handle = tokio::runtime::Handle::current();
    let handle = runtime_handle.spawn(async move {
        println!("Task via handle running!");
        flag_clone.store(true, Ordering::SeqCst);
    });
    
    println!("After spawn, waiting...");
    sleep(Duration::from_millis(100)).await;
    
    println!("Flag: {}", flag.load(Ordering::SeqCst));
    assert!(flag.load(Ordering::SeqCst), "Handle spawn should work");
}

#[cfg(feature = "async")]
#[tokio::test] 
async fn test_spawn_from_sync_context() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::time::sleep;
    use std::time::Duration;
    
    let flag = Arc::new(AtomicBool::new(false));
    
    // Simulate spawning from sync context
    fn spawn_from_sync(flag: Arc<AtomicBool>) {
        println!("In sync context, trying to spawn");
        let runtime_handle = tokio::runtime::Handle::current();
        let _guard = runtime_handle.enter();
        
        tokio::spawn(async move {
            println!("Task from sync context running!");
            flag.store(true, Ordering::SeqCst);
        });
        println!("Spawned from sync context");
    }
    
    spawn_from_sync(flag.clone());
    
    println!("Back in async, waiting...");
    sleep(Duration::from_millis(100)).await;
    
    println!("Flag: {}", flag.load(Ordering::SeqCst));
    assert!(flag.load(Ordering::SeqCst), "Sync context spawn should work");
}