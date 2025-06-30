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

fn increment(cx: &mut Syzygy<TestModel>) {
    cx.update(|m| {
        m.counter += 1;
    });
}

#[cfg(not(feature = "parallel"))]
#[cfg(feature = "async")]
#[tokio::test]
async fn test_async_dispatch() {
    let model = TestModel { counter: 0 };
    let mut syzygy: Syzygy<TestModel> = Syzygy::builder().model(model).build();

    // Dispatch the effect multiple times
    for _ in 0..5 {
        syzygy.dispatch(|cx: &mut Syzygy<TestModel>| increment(cx));
    }

    syzygy.handle_effects();

    assert_eq!(syzygy.model().counter, 5);
}

#[cfg(not(feature = "parallel"))]
#[cfg(feature = "async")]
#[tokio::test]
async fn test_sync_dispatch() {
    let model = TestModel { counter: 0 };
    let mut syzygy: Syzygy<TestModel> = Syzygy::builder().model(model).build();

    let rx = syzygy.dispatch_sync(increment);

    syzygy.handle_effects();
    rx.await.unwrap();

    assert_eq!(syzygy.model().counter, 1);
}