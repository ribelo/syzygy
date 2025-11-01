//! Demonstrates using separate async executors for IO and CPU work under a Tokio runtime.
//!
//! Run with:
//! ```bash
//! cargo run --example two_executors --features examples
//! ```

use std::marker::PhantomData;
use std::time::Duration;

use futures_util::future::BoxFuture;
use syzygy::executor::{AsyncExecutor, ExecutorError, ExecutorLifecycle, Task, TokioExecutor};
use syzygy::prelude::*;

#[derive(Debug, Default)]
struct DemoModel {
    io_message: Option<String>,
    cpu_result: Option<u128>,
}

#[derive(Debug, Clone)]
enum DemoEvent {
    Start,
    IoFinished(String),
    CpuFinished(u128),
}

#[derive(Debug, Clone)]
enum DemoEffect {
    FetchGreeting,
    CrunchNumber(u64),
}

fn event_handler(event: DemoEvent, model: &mut DemoModel) -> Command<DemoEvent, DemoEffect> {
    match event {
        DemoEvent::Start => on_start(),
        DemoEvent::IoFinished(message) => on_io_finished(model, message),
        DemoEvent::CpuFinished(value) => on_cpu_finished(model, value),
    }
}

fn on_start() -> Command<DemoEvent, DemoEffect> {
    cmd::parallel([DemoEffect::FetchGreeting, DemoEffect::CrunchNumber(38)])
}

fn on_io_finished(model: &mut DemoModel, message: String) -> Command<DemoEvent, DemoEffect> {
    model.io_message = Some(message);
    cmd::none()
}

fn on_cpu_finished(model: &mut DemoModel, value: u128) -> Command<DemoEvent, DemoEffect> {
    model.cpu_result = Some(value);
    cmd::none()
}

#[derive(Debug, Default)]
struct IoRuntimeTag;

#[derive(Debug, Default)]
struct CpuRuntimeTag;

type IoRuntime = TaggedTokioExecutor<IoRuntimeTag>;
type CpuRuntime = TaggedTokioExecutor<CpuRuntimeTag>;

#[derive(Debug)]
struct TaggedTokioExecutor<Tag> {
    inner: TokioExecutor,
    _marker: PhantomData<Tag>,
}

impl<Tag> TaggedTokioExecutor<Tag> {
    fn from_executor(inner: TokioExecutor) -> Self {
        Self {
            inner,
            _marker: PhantomData,
        }
    }
}

impl TaggedTokioExecutor<IoRuntimeTag> {
    fn new(worker_threads: usize) -> Self {
        Self::from_executor(TokioExecutor::multi_thread_io("io-pool", worker_threads))
    }
}

impl TaggedTokioExecutor<CpuRuntimeTag> {
    fn new(worker_threads: usize) -> Self {
        Self::from_executor(TokioExecutor::multi_thread_cpu("cpu-pool", worker_threads))
    }
}

impl<Tag> ExecutorLifecycle for TaggedTokioExecutor<Tag>
where
    Tag: Send + Sync + 'static,
{
    fn shutdown(&self) {
        self.inner.shutdown();
    }

    fn wait(&self) {
        self.inner.wait();
    }
}

impl<Tag> AsyncExecutor for TaggedTokioExecutor<Tag>
where
    Tag: Send + Sync + 'static,
{
    fn spawn_async(&self, job: BoxFuture<'static, ()>) -> Result<(), ExecutorError> {
        <TokioExecutor as AsyncExecutor>::spawn_async(&self.inner, job)
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        <TokioExecutor as AsyncExecutor>::sleep(&self.inner, duration)
    }
}

fn effect_handler(effect: DemoEffect, _resources: ()) -> Task<DemoEvent, DemoEffect> {
    match effect {
        DemoEffect::FetchGreeting => fetch_greeting(),
        DemoEffect::CrunchNumber(n) => crunch_number(n),
    }
}

fn fetch_greeting() -> Task<DemoEvent, DemoEffect> {
    Task::async_on::<IoRuntime, _>(async move {
        tokio::time::sleep(Duration::from_millis(40)).await;
        cmd::event(DemoEvent::IoFinished(format!(
            "hello from thread {:?}",
            std::thread::current().id()
        )))
    })
}

fn crunch_number(n: u64) -> Task<DemoEvent, DemoEffect> {
    Task::async_on::<CpuRuntime, _>(async move {
        let result = fibonacci(n);
        cmd::event(DemoEvent::CpuFinished(result))
    })
}

fn fibonacci(n: u64) -> u128 {
    match n {
        0 => 0,
        1 => 1,
        _ => {
            let mut prev = 0u128;
            let mut curr = 1u128;
            for _ in 2..=n {
                let next = prev + curr;
                prev = curr;
                curr = next;
            }
            curr
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<DemoEvent, DemoEffect>()
        .model(DemoModel::default())
        .event_handler(event_handler)
        .effect_handler(effect_handler)
        .profile_server()
        .with_async_executor(IoRuntime::new(2))
        .with_async_executor(CpuRuntime::new(2))
        .build();

    runner
        .core()
        .try_send_event(DemoEvent::Start)
        .expect("event channel should be open");

    runner.run_until(|core, _| {
        let model = core.model();
        model.io_message.is_some() && model.cpu_result.is_some()
    })?;

    let model = runner.core().model();
    println!("IO result: {:?}", model.io_message);
    println!("CPU result: {:?}", model.cpu_result);
    Ok(())
}
