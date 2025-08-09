use syzygy::prelude::*;

#[derive(Debug, Clone)]
struct TestModel {
    counter: i32,
}

impl Model for TestModel {
    type Snapshot = Self;
    fn to_snapshot(&self) -> Self::Snapshot {
        self.clone()
    }
}

fn increment(cx: &mut Syzygy<TestModel, ()>) {
    cx.update(|m| {
        m.counter += 1;
    });
}

#[cfg(not(feature = "parallel"))]
#[cfg(feature = "async")]
#[tokio::test(flavor = "multi_thread")]
async fn test_thread_task() {
    let model = TestModel { counter: 0 };
    let mut cx: Syzygy<TestModel, ()> = Syzygy::builder().model(model).build();

    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let (tx2, rx2) = tokio::sync::oneshot::channel();

    cx.spawn_blocking(move |cx| {
        println!("hello from thread task 1");
        cx.dispatch_closure(increment);
        let _ = tx1.send(());
    });

    cx.spawn_blocking(move |cx| {
        println!("hello from thread task 2");
        cx.dispatch_closure(increment);
        let _ = tx2.send(());
    });

    // Process the spawn effects to actually start the tasks
    cx.handle_effects();

    // Wait for tasks to complete
    let _ = rx1.await;
    let _ = rx2.await;

    // Process any remaining effects from the tasks
    cx.handle_effects();

    assert_eq!(cx.model().counter, 2);
}

#[cfg(not(feature = "parallel"))]
#[cfg(feature = "async")]
#[tokio::test(flavor = "multi_thread")]
async fn test_async_task() {
    let model = TestModel { counter: 0 };
    let mut cx: Syzygy<TestModel, ()> = Syzygy::builder().model(model).build();

    let (tx1, rx1) = tokio::sync::oneshot::channel();
    let (tx2, rx2) = tokio::sync::oneshot::channel();

    cx.task(move |cx| async move {
        println!("hello from async task 1");
        cx.dispatch_closure(increment);
        let _ = tx1.send(());
    });

    cx.task(move |cx| async move {
        println!("hello from async task 2");
        cx.dispatch_closure(increment);
        let _ = tx2.send(());
    });

    // Process the task effects to actually spawn the tasks
    cx.handle_effects();

    // Wait for tasks to complete
    let _ = rx1.await;
    let _ = rx2.await;

    // Process any remaining effects from the tasks
    cx.handle_effects();

    assert_eq!(cx.model().counter, 2);
}
