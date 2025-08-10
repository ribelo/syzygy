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

#[derive(Debug)]
struct IncrementEvent;

impl Event<TestModel> for IncrementEvent {
    fn apply(self, cx: &mut Syzygy<TestModel, Self>) {
        cx.update(|m| {
            m.counter += 1;
        });
    }
}

#[cfg(not(feature = "parallel"))]
#[cfg(feature = "async")]
#[tokio::test]
async fn test_async_dispatch() {
    let model = TestModel { counter: 0 };
    let mut syzygy: Syzygy<TestModel, IncrementEvent> = Syzygy::builder().model(model).build();

    // Dispatch the effect multiple times
    for _ in 0..5 {
        syzygy.dispatch(IncrementEvent);
    }

    syzygy.handle_effects();

    assert_eq!(syzygy.model().counter, 5);
}

#[cfg(not(feature = "parallel"))]
#[cfg(feature = "async")]
#[tokio::test]
async fn test_event_dispatch() {
    let model = TestModel { counter: 0 };
    let mut syzygy: Syzygy<TestModel, IncrementEvent> = Syzygy::builder().model(model).build();

    syzygy.dispatch(IncrementEvent);
    syzygy.handle_effects();

    assert_eq!(syzygy.model().counter, 1);
}
