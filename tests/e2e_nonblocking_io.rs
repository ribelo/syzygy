#![cfg(feature = "tokio")]

use std::time::{Duration, Instant};

use syzygy::executor::{Task, TokioExecutor};
use syzygy::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Default)]
struct IoModel {
    short_done: bool,
    io_done: bool,
    short_elapsed_ms: Option<u128>,
    io_elapsed_ms: Option<u128>,
    total_bytes: usize,
}

#[derive(Debug, Clone)]
enum IoEvent {
    Start,
    ShortTaskDone(u128),
    LongIoDone { elapsed_ms: u128, bytes: usize },
}

#[derive(Debug, Clone)]
enum IoEffect {
    RunShortTask,
    RunLongIo,
}

fn update(event: IoEvent, model: &mut IoModel) -> Command<IoEvent, IoEffect> {
    match event {
        IoEvent::Start => Command::parallel([IoEffect::RunShortTask, IoEffect::RunLongIo]),
        IoEvent::ShortTaskDone(ms) => {
            model.short_done = true;
            model.short_elapsed_ms = Some(ms);
            Command::none()
        }
        IoEvent::LongIoDone { elapsed_ms, bytes } => {
            model.io_done = true;
            model.io_elapsed_ms = Some(elapsed_ms);
            model.total_bytes = bytes;
            Command::none()
        }
    }
}

fn effects(effect: IoEffect, _resources: ()) -> Task<IoEvent, IoEffect> {
    match effect {
        IoEffect::RunShortTask => run_short_task(),
        IoEffect::RunLongIo => run_long_io(),
    }
}

fn run_short_task() -> Task<IoEvent, IoEffect> {
    Task::<IoEvent, IoEffect>::async_owned::<TokioExecutor, _, _>(|_resources| async move {
        let t0 = Instant::now();
        tokio::time::sleep(Duration::from_millis(30)).await;
        Command::event(IoEvent::ShortTaskDone(t0.elapsed().as_millis()))
    })
}

fn run_long_io() -> Task<IoEvent, IoEffect> {
    Task::<IoEvent, IoEffect>::async_owned::<TokioExecutor, _, _>(|_resources| async move {
        let (mut reader, mut writer) = tokio::io::duplex(64 * 1024);
        let writer_fut = async move {
            let chunks = 20usize;
            let chunk_size = 50_000usize;
            let payload = vec![1u8; chunk_size];
            for _ in 0..chunks {
                writer.write_all(&payload).await.expect("write should succeed");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let _ = writer.shutdown().await;
        };

        let t0 = Instant::now();
        let mut total = 0usize;
        let reader_fut = async move {
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let n = reader.read(&mut buf).await.expect("read should succeed");
                if n == 0 {
                    break;
                }
                total += n;
            }
            total
        };

        let (_w, bytes) = tokio::join!(writer_fut, reader_fut);
        let elapsed = t0.elapsed().as_millis();
        Command::event(IoEvent::LongIoDone { elapsed_ms: elapsed, bytes })
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn e2e_nonblocking_io_with_syzygy() {
    let mut runner = Syzygy::builder::<IoEvent, IoEffect>()
        .model(IoModel::default())
        .event_handler(update)
        .effect_handler(effects)
        .with_async_executor(TokioExecutor::multi_thread_io("e2e-io", 2))
        .build();

    runner.core().send_event(IoEvent::Start);

    // First phase: short task should finish quickly while IO runs in background
    let short_elapsed = tokio::task::spawn_blocking({
        let mut runner = runner;
        move || {
            let start = Instant::now();
            while !runner.core().model().short_done {
                let _ = runner.step().expect("step should succeed");
            }
            (runner, start.elapsed())
        }
    })
    .await
    .expect("spawn_blocking failed");

    let (mut runner, short_elapsed_wall) = short_elapsed;
    assert!(
        short_elapsed_wall < Duration::from_millis(200),
        "short task took too long: {short_elapsed_wall:?}"
    );

    // Second phase: drain to IO completion
    let runner = tokio::task::spawn_blocking(move || {
        while !runner.core().model().io_done {
            let _ = runner.step().expect("step should succeed");
        }
        runner
    })
    .await
    .expect("spawn_blocking failed");

    let model = runner.core().model();
    // IO task should have run for a noticeable time and transferred expected data
    assert!(
        model.io_elapsed_ms.unwrap_or(0) >= 300,
        "IO finished suspiciously fast: {:?}ms",
        model.io_elapsed_ms
    );
    assert!(
        model.total_bytes >= 1_000_000,
        "Transferred too few bytes: {}",
        model.total_bytes
    );
}
