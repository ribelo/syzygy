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

#[cfg(feature = "async")]
#[tokio::test]
async fn test_model() {
    let model = TestModel { counter: 0 };
    let mut syzygy = Syzygy::builder().model(model).build();

    // Test initial state
    let counter = syzygy.model().counter;
    assert_eq!(counter, 0);

    // Test model() access
    assert_eq!(syzygy.model().counter, 0);

    // Test model_mut() modification
    syzygy.model_mut().counter += 1;
    assert_eq!(syzygy.model().counter, 1);

    // Test update
    syzygy.update(|m| {
        m.counter = 42;
    });
    assert_eq!(syzygy.model().counter, 42);

    // Test query
    let value = syzygy.query(|m| m.counter);
    assert_eq!(value, 42);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_increment_dispatch() {
    let model = TestModel { counter: 0 };
    let mut syzygy = Syzygy::builder().model(model).build();

    for _i in 0..5 {
        syzygy.dispatch(increment);
        syzygy.handle_effects();
    }

    assert_eq!(syzygy.model().counter, 5);
}

#[cfg(feature = "async")]
#[tokio::test]
async fn test_dispatch_performance() {
    let model = TestModel { counter: 0 };
    let mut syzygy = Syzygy::builder().model(model).build();

    use std::time::Instant;

    // Measure time to process 1000 dispatches
    let start = Instant::now();
    for _i in 0..1000 {
        syzygy.dispatch(increment);
    }

    // Process all effects
    let process_start = Instant::now();
    syzygy.handle_effects();
    let process_duration = process_start.elapsed();

    let total_duration = start.elapsed();

    assert_eq!(syzygy.model().counter, 1000);

    println!("Total time for 1000 dispatches: {:?}", total_duration);
    println!("Time to process effects: {:?}", process_duration);

    // Performance assertions - very generous to avoid flaky tests
    assert!(total_duration.as_millis() < 100, "Dispatch should be fast");
    assert!(process_duration.as_millis() < 50, "Effect processing should be fast");
}

// Benchmark: direct model update vs effect dispatch overhead
#[cfg(feature = "async")]
#[tokio::test]
async fn test_benchmark_direct_model_update() {
    let model = TestModel { counter: 0 };
    let mut syzygy = Syzygy::builder().model(model).build();

    use std::time::Instant;

    // Benchmark direct model updates
    let start = Instant::now();
    for _i in 0..1000 {
        syzygy.update(|m| m.counter += 1);
    }
    let direct_duration = start.elapsed();

    // Reset
    syzygy.model_mut().counter = 0;

    // Benchmark effect dispatch
    let start = Instant::now();
    for _i in 0..1000 {
        syzygy.dispatch(increment);
    }
    syzygy.handle_effects();
    let dispatch_duration = start.elapsed();

    println!("Direct update time for 1000 operations: {:?}", direct_duration);
    println!("Effect dispatch time for 1000 operations: {:?}", dispatch_duration);

    assert_eq!(syzygy.model().counter, 1000);

    // Effect dispatch should be reasonably close to direct update
    // Allow for some overhead but not too much
    let overhead_ratio = dispatch_duration.as_nanos() as f64 / direct_duration.as_nanos() as f64;
    println!("Overhead ratio: {:.2}x", overhead_ratio);

    // Very generous ratio to avoid flaky tests
    assert!(overhead_ratio < 100.0, "Effect dispatch shouldn't be more than 100x slower than direct updates");
}