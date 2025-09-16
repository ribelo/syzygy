//! Demonstrates scheduling an async effect on Tokio.
//!
//! Build with
//! ```bash
//! cargo run --example async_effect --features examples
//! ```

use std::time::Duration;

use syzygy::executor::{ExecutorRegistry, Outcome, Task, TokioExecutor};
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct DownloadModel {
    status: String,
    finished: bool,
}

#[derive(Debug, Clone)]
enum DownloadEvent {
    Start,
    Completed(String),
}

#[derive(Debug, Clone)]
enum DownloadEffect {
    FetchGreeting,
}

fn update_download(
    event: DownloadEvent,
    ctx: &mut EventContext<DownloadEvent, DownloadEffect, DownloadModel>,
) -> Command<DownloadEvent, DownloadEffect> {
    let model = ctx.model_mut();
    match event {
        DownloadEvent::Start => {
            model.status = "requesting...".to_string();
            Command::effect(DownloadEffect::FetchGreeting)
        }
        DownloadEvent::Completed(message) => {
            model.status = format!("response: {message}");
            model.finished = true;
            Command::none()
        }
    }
}

fn handle_download_effect(
    effect: DownloadEffect,
    _ctx: &EffectContext<DownloadEvent, ()>,
) -> Task<DownloadEvent, ()> {
    match effect {
        DownloadEffect::FetchGreeting => Task::async_task::<TokioExecutor, _>(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            Outcome::Events(vec![DownloadEvent::Completed(
                "hello from async effect".to_string(),
            )])
        }),
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = ExecutorRegistry::new();
    registry.insert_async(TokioExecutor::multi_thread_io("async-example", 2));

    let mut runner = Syzygy::builder::<DownloadEvent, DownloadEffect>()
        .model(DownloadModel::default())
        .event_handler(update_download)
        .effect_handler(handle_download_effect)
        .with_executor_registry(registry)
        .build_runner();

    runner.core().send_event(DownloadEvent::Start);

    runner.run_until(
        |core, _shell| core.model().finished,
        syzygy::scheduler::scheduler(),
    )?;

    println!("{}", runner.core().model().status);
    Ok(())
}
