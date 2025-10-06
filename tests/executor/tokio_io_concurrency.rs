#![cfg(feature = "tokio")]

use std::time::{Duration, Instant};

use syzygy::executor::{AsyncOwnedExecutor, ExecutorLifecycle, TokioExecutor};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::oneshot;

type Event = ();

#[tokio::test(flavor = "multi_thread")]
async fn nonblocking_io_allows_other_tasks_to_progress() {
    // Create a dedicated IO runtime with multiple threads
    let exec = TokioExecutor::multi_thread_io("tokio-io-nonblocking", 2);

    // In-memory duplex stream simulating a bidirectional IO channel
    // This avoids relying on OS networking while still exercising async IO.
    let (mut rx_end, mut tx_end) = tokio::io::duplex(64 * 1024);

    // Signal when the long-running IO completes
    let (io_done_tx, io_done_rx) = oneshot::channel::<usize>();
    // Signal when a short, unrelated task completes
    let (short_done_tx, short_done_rx) = oneshot::channel::<()>();

    // Long-running async IO: read N chunks with the writer injecting a delay between sends
    <TokioExecutor as AsyncOwnedExecutor<Event>>::spawn_owned(&exec, move |_| async move {
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
    })
    .expect("spawn reader should succeed");

    // Writer: send data in chunks with intentional delay to simulate IO pacing
    let writer = async move {
        let chunks = 20usize;
        let chunk_size = 50_000usize;
        let payload = vec![1u8; chunk_size];
        for _ in 0..chunks {
            tx_end
                .write_all(&payload)
                .await
                .expect("write should succeed");
            // Simulate network pacing; if IO were blocking, this could starve other tasks
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let _ = tx_end.shutdown().await; // finish the stream
    };

    <TokioExecutor as AsyncOwnedExecutor<Event>>::spawn_owned(&exec, move |_| writer)
        .expect("spawn writer should succeed");

    // Short task that should complete quickly even while IO is active
    <TokioExecutor as AsyncOwnedExecutor<Event>>::spawn_owned(&exec, move |_| async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = short_done_tx.send(());
    })
    .expect("spawn short task should succeed");

    let start = Instant::now();

    // The short task should complete well before the long IO finishes
    tokio::time::timeout(Duration::from_millis(150), short_done_rx)
        .await
        .expect("short task should not be delayed by IO")
        .expect("channel should deliver signal");

    // The long IO should complete with expected volume and after a noticeable duration
    let total_bytes = tokio::time::timeout(Duration::from_secs(5), io_done_rx)
        .await
        .expect("IO task should complete")
        .expect("channel should deliver byte count");

    let elapsed = start.elapsed();
    assert!(
        elapsed >= Duration::from_millis(350),
        "IO finished suspiciously fast; elapsed={elapsed:?}"
    );
    assert!(
        total_bytes >= 1_000_000,
        "Reader received too few bytes: {total_bytes}"
    );

    exec.shutdown();
    exec.join().await;
}

