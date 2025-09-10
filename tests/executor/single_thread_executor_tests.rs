use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use futures_util::future::FutureExt;
use syzygy::executor::{ExecutorError, ExecutorLifecycle, SingleThreadExecutor, SyncExecutor};
use syzygy::streaming::EffectOutput;

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum TestEvent {
    A(usize),
    B(&'static str),
}

// 1) FIFO ordering under deep queues (targeted, smaller than stress test)
#[tokio::test]
async fn single_thread_executor_executes_strict_fifo_under_deep_queue() {
    let exec = SingleThreadExecutor::new();
    let total = 1000usize;
    let completion = Arc::new(Mutex::new(Vec::with_capacity(total)));

    let mut handles = Vec::with_capacity(total);
    for i in 0..total {
        let order = Arc::clone(&completion);
        let h = exec.spawn_sync(Box::new(move || {
            order.lock().unwrap().push(i);
            EffectOutput::Future(async move { vec![TestEvent::A(i)] }.boxed())
        }));
        handles.push(h);
    }

    // Await all joins
    let results = futures::future::join_all(handles).await;
    assert!(
        results.iter().all(std::result::Result::is_ok),
        "all jobs should complete successfully"
    );

    // Verify FIFO order of completion (worker is single-threaded, channel FIFO)
    let order = completion.lock().unwrap();
    assert_eq!(order.len(), total);
    for (idx, val) in order.iter().enumerate() {
        assert_eq!(*val, idx);
    }
}

// 2) Panic isolation maps to ExecutorError::Panic and doesn't poison the worker
#[tokio::test]
async fn single_thread_executor_maps_panic_to_error_and_continues() {
    let exec = SingleThreadExecutor::new();

    // Job that panics with a string
    let err = exec
        .spawn_sync(Box::new(|| -> EffectOutput<TestEvent> {
            panic!("just exploding");
        }))
        .await
        .expect_err("panic should map to error");

    match err {
        ExecutorError::Panic { msg } => assert_eq!(msg, "just exploding"),
        other => panic!("expected Panic error, got {other:?}"),
    }

    // Subsequent job still executes successfully (isolation)
    let ok = exec
        .spawn_sync(Box::new(|| {
            EffectOutput::Future(async { vec![TestEvent::B("ok")] }.boxed())
        }))
        .await;
    assert!(ok.is_ok(), "executor should continue after a panic");

    exec.join().await;
}

// 3) Long-running job does not cause starvation (later tasks eventually run)
#[tokio::test]
async fn single_thread_executor_long_running_job_does_not_starve_followers() {
    let exec = SingleThreadExecutor::new();

    // Long job first
    let long_started = Arc::new(AtomicBool::new(false));
    let long_started_cl = Arc::clone(&long_started);
    let long = exec.spawn_sync(Box::new(move || {
        long_started_cl.store(true, Ordering::SeqCst);
        // Simulate CPU work ~50ms
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_millis(50) {
            std::hint::spin_loop();
        }
        EffectOutput::<TestEvent>::None
    }));

    // Short job queued behind should still complete soon after the long one
    let short_done = Arc::new(AtomicBool::new(false));
    let short_done_cl = Arc::clone(&short_done);
    let short = exec.spawn_sync(Box::new(move || {
        short_done_cl.store(true, Ordering::SeqCst);
        EffectOutput::<TestEvent>::None
    }));

    // Ensure the long job actually started
    tokio::time::timeout(Duration::from_secs(1), async {
        while !long_started.load(Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("long job should start");

    // Both should complete within a reasonable timeout
    tokio::time::timeout(Duration::from_secs(2), long)
        .await
        .expect("long join")
        .expect("long ok");
    tokio::time::timeout(Duration::from_secs(2), short)
        .await
        .expect("short join")
        .expect("short ok");

    assert!(
        short_done.load(Ordering::SeqCst),
        "short job should have completed (no starvation)"
    );
}

// 4) Shutdown/join semantics: join waits for queued work and prevents new jobs
#[tokio::test]
async fn single_thread_executor_shutdown_and_join_semantics() {
    let exec = SingleThreadExecutor::new();

    // Queue a few quick jobs
    let mut handles = Vec::new();
    for _ in 0..10 {
        handles.push(exec.spawn_sync(Box::new(|| {
            EffectOutput::Future(async { Vec::<TestEvent>::new() }.boxed())
        })));
    }

    // Initiate shutdown and await join (should process queued jobs first)
    let join_res = tokio::time::timeout(Duration::from_secs(2), exec.join()).await;
    assert!(join_res.is_ok(), "join should not hang");

    // All previously queued jobs should have finished (either Ok(None-like) or Ok(Future))
    for h in handles {
        let _ = h.await.expect("pre-shutdown job should succeed");
    }

    // New spawns after join should be rejected
    let res = exec
        .spawn_sync(Box::new(|| EffectOutput::<TestEvent>::None))
        .await;
    assert!(
        matches!(res, Err(ExecutorError::WorkerGone)),
        "should not accept new work after shutdown"
    );
}
