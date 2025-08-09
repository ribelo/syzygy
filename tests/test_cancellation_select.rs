#[cfg(feature = "async")]
#[tokio::test]
async fn test_select_with_cancellation() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::time::sleep;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;
    
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();
    let shutdown = CancellationToken::new();
    
    println!("Is cancelled? {}", shutdown.is_cancelled());
    
    tokio::spawn(async move {
        println!("Task started");
        tokio::select! {
            _ = shutdown.cancelled() => {
                println!("Task cancelled");
            }
            _ = async {
                println!("Inside future");
                flag_clone.store(true, Ordering::SeqCst);
                println!("Future completed");
            } => {
                println!("Future branch taken");
            }
        }
        println!("After select");
    });
    
    println!("Spawned, waiting...");
    sleep(Duration::from_millis(100)).await;
    
    println!("Flag: {}", flag.load(Ordering::SeqCst));
    assert!(flag.load(Ordering::SeqCst), "Select should work");
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_select_with_future_parameter() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio::time::sleep;
    use std::time::Duration;
    use tokio_util::sync::CancellationToken;
    use std::future::Future;
    
    async fn run_with_cancellation<F>(shutdown: CancellationToken, future: F)
    where
        F: Future<Output = ()>,
    {
        println!("run_with_cancellation started");
        tokio::select! {
            _ = shutdown.cancelled() => {
                println!("Cancelled!");
            }
            _ = future => {
                println!("Future completed!");
            }
        }
    }
    
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = flag.clone();
    let shutdown = CancellationToken::new();
    
    tokio::spawn(async move {
        run_with_cancellation(shutdown, async move {
            println!("User future running");
            flag_clone.store(true, Ordering::SeqCst);
        }).await;
    });
    
    sleep(Duration::from_millis(100)).await;
    println!("Flag: {}", flag.load(Ordering::SeqCst));
    assert!(flag.load(Ordering::SeqCst), "Parameterized future should work");
}