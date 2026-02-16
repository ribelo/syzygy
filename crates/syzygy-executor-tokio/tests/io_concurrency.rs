use std::time::{Duration, Instant};

use futures_util::FutureExt;
use syzygy::executor::{AsyncExecutor, ExecutorLifecycle};
use syzygy_executor_tokio::TokioExecutor;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

#[tokio::test(flavor = "multi_thread")]
async fn nonblocking_io_allows_other_tasks_to_progress() {
    let exec = TokioExecutor::multi_thread_io("tokio-io-nonblocking", 2);

    let (rx_stream, tx_stream) = tokio::io::duplex(64 * 1024);
    let (io_done_tx, io_done_rx) = oneshot::channel::<usize>();
    let (short_done_tx, short_done_rx) = oneshot::channel::<()>();

    let reader = async move {
        let mut rx_end = rx_stream;
        let mut total = 0usize;
        let mut buf = vec![0u8; 16 * 1024];
        while total < 1_000_000 {
            let n = rx_end.read(&mut buf).await.expect("read should succeed");
            if n == 0 {
                break;
            }
            total += n;
        }
        let _ = io_done_tx.send(total);
    };
    <TokioExecutor as AsyncExecutor>::spawn_async(&exec, reader.boxed())
        .expect("spawn reader should succeed");

    let writer = async move {
        let mut tx_end = tx_stream;
        let payload = vec![1u8; 50_000];
        for _ in 0..20 {
            tx_end
                .write_all(&payload)
                .await
                .expect("write should succeed");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let _ = tx_end.shutdown().await;
    };
    <TokioExecutor as AsyncExecutor>::spawn_async(&exec, writer.boxed())
        .expect("spawn writer should succeed");

    let short_task = async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = short_done_tx.send(());
    };
    <TokioExecutor as AsyncExecutor>::spawn_async(&exec, short_task.boxed())
        .expect("spawn short task should succeed");

    let start = Instant::now();

    tokio::time::timeout(Duration::from_millis(150), short_done_rx)
        .await
        .expect("short task should not be delayed by IO")
        .expect("channel should deliver signal");

    let total_bytes = tokio::time::timeout(Duration::from_secs(5), io_done_rx)
        .await
        .expect("IO task should complete")
        .expect("channel should deliver byte count");

    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(350),
        "IO finished suspiciously fast"
    );
    assert!(
        total_bytes >= 1_000_000,
        "Reader received too few bytes: {total_bytes}"
    );

    exec.shutdown();
    exec.wait();
}
