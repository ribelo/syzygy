#![cfg(feature = "shell")]
#![allow(clippy::needless_pass_by_value)]
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use syzygy::executor::Task;
use syzygy::prelude::*;
use syzygy_executor_single::SingleThreadExecutor;

// --- Predictable Event Processing (FIFO) ---

#[derive(Debug, Default)]
struct OrderModel {
    seq: Vec<i32>,
}

#[derive(Debug, Clone)]
enum OrderEvent {
    Push(i32),
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum OrderEffect {
    None,
}

fn fifo_update(e: OrderEvent, m: &mut OrderModel) -> Command<OrderEvent, OrderEffect> {
    match e {
        OrderEvent::Push(n) => {
            m.seq.push(n);
            Command::none()
        }
    }
}

#[test]
fn fifo_event_order_is_preserved() {
    let (mut core, _tx) = syzygy::core::Core::new(fifo_update, OrderModel::default());
    // Enqueue a known order
    for n in 0..5 {
        core.try_send_event(OrderEvent::Push(n))
            .expect("event channel should be open");
    }
    let _ = core.process_events();
    assert_eq!(core.model().seq, vec![0, 1, 2, 3, 4]);
}

// --- Resources are cloned per effect invocation ---

#[derive(Debug)]
struct CountedResources {
    clones: Arc<AtomicUsize>,
}

impl Clone for CountedResources {
    fn clone(&self) -> Self {
        self.clones.fetch_add(1, Ordering::SeqCst);
        Self {
            clones: Arc::clone(&self.clones),
        }
    }
}

#[derive(Debug, Default)]
struct CloneModel {
    hits: usize,
}

#[derive(Debug, Clone)]
enum CloneEvent {
    Trigger,
    Done,
}

#[derive(Debug, Clone)]
enum CloneEffect {
    DoOne,
}

fn clone_update(e: CloneEvent, m: &mut CloneModel) -> Command<CloneEvent, CloneEffect> {
    match e {
        CloneEvent::Trigger => Command::effects(vec![CloneEffect::DoOne, CloneEffect::DoOne]),
        CloneEvent::Done => {
            m.hits += 1;
            Command::none()
        }
    }
}

fn clone_effects(_x: CloneEffect, _r: CountedResources) -> Task<CloneEvent, CloneEffect> {
    // No executors needed; run on current runtime if present, else block inline
    Task::blocking_with_resource_on::<SingleThreadExecutor<()>, (), _>(|_| {
        Command::event(CloneEvent::Done)
    })
}

#[tokio::test(flavor = "current_thread")]
async fn resources_cloned_per_effect() {
    let counter = Arc::new(AtomicUsize::new(0));
    let resources = CountedResources {
        clones: Arc::clone(&counter),
    };

    let mut app = Syzygy::builder::<CloneEvent, CloneEffect>()
        .model(CloneModel::default())
        .with_resources(resources)
        .event_handler(clone_update)
        .effect_handler(clone_effects)
        .with_resource_blocking_executor(SingleThreadExecutor::new())
        .build();

    // Baseline clone count before any effects
    let before = counter.load(Ordering::SeqCst);

    app.core()
        .try_send_event(CloneEvent::Trigger)
        .expect("event channel should be open");
    // Drain until both DoOne effects complete and emit two Done events
    app.drain_until(|m: &CloneModel| m.hits == 2, Duration::from_secs(1))
        .unwrap();

    let after = counter.load(Ordering::SeqCst);
    // Expect exactly two resource clones for two effect invocations
    assert_eq!(after - before, 2);
}
