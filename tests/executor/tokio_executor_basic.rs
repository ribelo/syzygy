#![cfg(feature = "tokio")]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use syzygy::executor::{AsyncOwnedExecutor, ExecutorError, ExecutorLifecycle, TokioExecutor};
use tokio::sync::{Barrier, oneshot};

type TestEvent = ();

#[tokio::test(flavor = "multi_thread")]
async fn spawn_owned_executes_job() {
    let executor = Arc::new(
        TokioExecutor::builder()
            .name("tokio-basic")
            .multi_thread()
            .worker_threads(2)
            .io()
            .build(),
    );
    let (tx, rx) = oneshot::channel();

    <TokioExecutor as AsyncOwnedExecutor<TestEvent>>::spawn_owned(
        &*executor,
        move |_| async move {
            let _ = tx.send(123u32);
        },
    )
    .expect("spawn should succeed");

    let value = tokio::time::timeout(Duration::from_secs(1), rx)
        .await
        .expect("job should complete")
        .expect("channel should deliver value");
    assert_eq!(value, 123);

    executor.shutdown();
    executor.join().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_owned_runs_jobs_concurrently() {
    let executor = TokioExecutor::multi_thread_io("tokio-concurrent", 4);
    let barrier = Arc::new(Barrier::new(4));
    let completion = Arc::new(Mutex::new(Vec::new()));

    let start = Instant::now();
    for id in 0..4 {
        let barrier_cl = Arc::clone(&barrier);
        let completion_cl = Arc::clone(&completion);
        <TokioExecutor as AsyncOwnedExecutor<TestEvent>>::spawn_owned(
            &executor,
            move |_| async move {
                barrier_cl.wait().await;
                tokio::time::sleep(Duration::from_millis(40 - (id * 5) as u64)).await;
                completion_cl.lock().unwrap().push(id);
            },
        )
        .expect("spawn should succeed");
    }

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if completion.lock().unwrap().len() == 4 {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("all jobs should finish");

    executor.shutdown();
    executor.join().await;

    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_millis(120),
        "jobs ran serially: {elapsed:?}"
    );

    let mut seen = completion.lock().unwrap().clone();
    seen.sort_unstable();
    assert_eq!(seen, vec![0, 1, 2, 3]);
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_after_shutdown_returns_error() {
    let executor = TokioExecutor::current_thread_io("tokio-shutdown");
    executor.shutdown();

    let result =
        <TokioExecutor as AsyncOwnedExecutor<TestEvent>>::spawn_owned(&executor, |_| async move {});
    assert!(matches!(result, Err(ExecutorError::WorkerGone)));

    executor.join().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sleep_delegates_to_runtime() {
    let executor = TokioExecutor::current_thread_io("tokio-sleep");

    let start = tokio::time::Instant::now();
    <TokioExecutor as AsyncOwnedExecutor<TestEvent>>::sleep(&executor, Duration::from_millis(20))
        .await;
    assert!(start.elapsed() >= Duration::from_millis(20));

    executor.shutdown();
    executor.join().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn try_from_current_creates_executor() {
    let executor = TokioExecutor::try_from_current().expect("tokio runtime should be available");
    let (tx, rx) = oneshot::channel();

    <TokioExecutor as AsyncOwnedExecutor<TestEvent>>::spawn_owned(&executor, move |_| async move {
        let _ = tx.send(());
    })
    .expect("spawn should succeed");

    tokio::time::timeout(Duration::from_secs(1), rx)
        .await
        .expect("job should complete")
        .expect("channel should deliver value");

    executor.shutdown();
    executor.join().await;
}
