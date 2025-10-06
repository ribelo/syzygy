//! Demonstrates using separate async executors for IO and CPU work under a Tokio runtime.
//!
//! Run with:
//! ```bash
//! cargo run --example two_executors --features examples
//! ```

use std::marker::PhantomData;
use std::time::Duration;

use futures_util::future::BoxFuture;
use syzygy::executor::{
    AsyncOwnedExecutor, ExecutorError, ExecutorLifecycle, Outcome, Task, TokioExecutor,
};
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

fn event_handler(
    event: DemoEvent,
    ctx: &mut EventContext<DemoEvent, DemoEffect, DemoModel>,
) -> Command<DemoEvent, DemoEffect> {
    match event {
        DemoEvent::Start => on_start(),
        DemoEvent::IoFinished(message) => on_io_finished(ctx.model_mut(), message),
        DemoEvent::CpuFinished(value) => on_cpu_finished(ctx.model_mut(), value),
    }
}

fn on_start() -> Command<DemoEvent, DemoEffect> {
    Command::parallel([DemoEffect::FetchGreeting, DemoEffect::CrunchNumber(38)])
}

fn on_io_finished(model: &mut DemoModel, message: String) -> Command<DemoEvent, DemoEffect> {
    model.io_message = Some(message);
    Command::none()
}

fn on_cpu_finished(model: &mut DemoModel, value: u128) -> Command<DemoEvent, DemoEffect> {
    model.cpu_result = Some(value);
    Command::none()
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

    fn join(&self) -> BoxFuture<'static, ()> {
        self.inner.join()
    }
}

impl<E, Tag> AsyncOwnedExecutor<E> for TaggedTokioExecutor<Tag>
where
    E: Send + Sync + 'static,
    Tag: Send + Sync + 'static,
{
    type Resources = ();

    fn spawn_owned<F, Fut>(&self, job: F) -> Result<(), ExecutorError>
    where
        F: FnOnce(Self::Resources) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        <TokioExecutor as AsyncOwnedExecutor<E>>::spawn_owned::<F, Fut>(&self.inner, job)
    }

    fn sleep(&self, duration: Duration) -> BoxFuture<'static, ()> {
        <TokioExecutor as AsyncOwnedExecutor<E>>::sleep(&self.inner, duration)
    }
}

fn effect_handler(effect: DemoEffect, _ctx: EffectContext<DemoEvent>) -> Task<DemoEvent> {
    match effect {
        DemoEffect::FetchGreeting => fetch_greeting(),
        DemoEffect::CrunchNumber(n) => crunch_number(n),
    }
}

fn fetch_greeting() -> Task<DemoEvent> {
    Task::async_owned::<IoRuntime, _, _>(move |_ctx, _resources| async move {
        tokio::time::sleep(Duration::from_millis(40)).await;
        Outcome::Event(DemoEvent::IoFinished(format!(
            "hello from thread {:?}",
            std::thread::current().id()
        )))
    })
}

fn crunch_number(n: u64) -> Task<DemoEvent> {
    Task::async_owned::<CpuRuntime, _, _>(move |_ctx, _resources| async move {
        let result = fibonacci(n);
        Outcome::Event(DemoEvent::CpuFinished(result))
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
        .with_async_executor(IoRuntime::new(2))
        .with_async_executor(CpuRuntime::new(2))
        .build();

    runner.core().send_event(DemoEvent::Start);

    runner.run_until(|core, _| {
        let model = core.model();
        model.io_message.is_some() && model.cpu_result.is_some()
    })?;

    let model = runner.core().model();
    println!("IO result: {:?}", model.io_message);
    println!("CPU result: {:?}", model.cpu_result);
    Ok(())
}
