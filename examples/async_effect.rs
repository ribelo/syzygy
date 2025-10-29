//! Demonstrates scheduling an async effect on Tokio.
//!
//! Build with
//! ```bash
//! cargo run --example async_effect --features examples
//! ```

use std::time::Duration;

use syzygy::executor::{Task, TokioExecutor};
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

fn event_handler(
    event: DownloadEvent,
    model: &mut DownloadModel,
) -> Command<DownloadEvent, DownloadEffect> {
    match event {
        DownloadEvent::Start => on_start(model),
        DownloadEvent::Completed(message) => on_completed(model, message),
    }
}

fn on_start(model: &mut DownloadModel) -> Command<DownloadEvent, DownloadEffect> {
    model.status = "requesting...".to_string();
    Command::effect(DownloadEffect::FetchGreeting)
}

fn on_completed(
    model: &mut DownloadModel,
    message: String,
) -> Command<DownloadEvent, DownloadEffect> {
    model.status = format!("response: {message}");
    model.finished = true;
    Command::none()
}

fn effect_handler(effect: DownloadEffect, _resources: ()) -> Task<DownloadEvent, DownloadEffect> {
    match effect {
        DownloadEffect::FetchGreeting => fetch_greeting_task(),
    }
}

fn fetch_greeting_task() -> Task<DownloadEvent, DownloadEffect> {
    Task::async_on::<TokioExecutor, _>(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Command::event(DownloadEvent::Completed(
            "hello from async effect".to_string(),
        ))
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<DownloadEvent, DownloadEffect>()
        .model(DownloadModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .with_async_executor(TokioExecutor::multi_thread_io("async-example", 2))
        .build();

    runner
        .core()
        .try_send_event(DownloadEvent::Start)
        .expect("event channel should be open");

    runner.run_until(|core, _shell| core.model().finished)?;

    println!("{}", runner.core().model().status);
    Ok(())
}
