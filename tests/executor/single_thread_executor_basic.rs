use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use syzygy::executor::{
    ExecutorError, ExecutorLifecycle, SingleThreadExecutor, SyncBorrowedExecutor,
};

type Event = ();

type Resources = Arc<Mutex<Vec<usize>>>;

type Exec = SingleThreadExecutor<Resources>;

fn spawn_job<F>(executor: &Exec, job: F) -> Result<(), ExecutorError>
where
    F: FnOnce(&mut Resources) + Send + 'static,
{
    <Exec as SyncBorrowedExecutor<Event>>::spawn_sync(executor, job)
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_sync_executes_jobs_in_order() {
    let shared: Resources = Arc::new(Mutex::new(Vec::new()));
    let executor = Arc::new(Exec::with_resources(shared.clone()));

    for idx in 0..5 {
        let exec_cl = Arc::clone(&executor);
        spawn_job(&exec_cl, move |resources| {
            let shared = Arc::clone(&*resources);
            shared.lock().unwrap().push(idx);
        })
        .expect("spawn should succeed");
    }

    executor.shutdown();
    executor.join().await;

    assert_eq!(*shared.lock().unwrap(), vec![0, 1, 2, 3, 4]);
}

#[tokio::test(flavor = "multi_thread")]
async fn spawn_sync_rejects_after_shutdown() {
    let shared: Resources = Arc::new(Mutex::new(Vec::new()));
    let executor = Exec::with_resources(shared);
    executor.shutdown();

    let result = spawn_job(&executor, |_resources| {});
    assert!(matches!(result, Err(ExecutorError::WorkerGone)));

    executor.join().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_waits_for_inflight_job() {
    let shared: Resources = Arc::new(Mutex::new(Vec::new()));
    let executor = Exec::with_resources(shared);
    let flag = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&flag);

    spawn_job(&executor, move |_resources| {
        std::thread::sleep(Duration::from_millis(50));
        flag_clone.store(true, Ordering::SeqCst);
    })
    .expect("spawn should succeed");

    executor.shutdown();
    executor.join().await;

    assert!(flag.load(Ordering::SeqCst));
}

#[tokio::test(flavor = "multi_thread")]
async fn panicking_job_does_not_poison_executor() {
    let shared: Resources = Arc::new(Mutex::new(Vec::new()));
    let executor = Exec::with_resources(shared.clone());

    spawn_job(&executor, |_resources| panic!("boom"))
        .expect("panic happens on worker thread, spawn succeeds");

    spawn_job(&executor, |resources| {
        let shared = Arc::clone(&*resources);
        shared.lock().unwrap().push(1);
    })
    .expect("executor should continue after panic");

    executor.shutdown();
    executor.join().await;

    assert_eq!(*shared.lock().unwrap(), vec![1]);
}
