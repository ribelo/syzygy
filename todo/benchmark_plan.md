# Syzygy Benchmarking Plan

Because you can't optimize what you can't measure, and your "it feels fast" is worth jack shit.

## Why Benchmark This Mess?

Before you unfuck your library, you need to know:
- What's actually slow (not what you think is slow)
- Whether your "optimizations" make things worse
- If you're introducing performance regressions
- Real numbers to put in your README instead of "blazingly fast" bullshit

## Setting Up Criterion (Not Criterium, You Illiterate Fuck)

### Dependencies

```toml
[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
# For async benchmarks if needed
tokio = { version = "1", features = ["full"] }
futures = "0.3"

[[bench]]
name = "syzygy_benchmarks"
harness = false
```

### Directory Structure

```
syzygy/
├── benches/
│   ├── syzygy_benchmarks.rs    # Main benchmark entry
│   ├── model_benchmarks.rs      # Model operations
│   ├── dispatch_benchmarks.rs   # Effect dispatching
│   ├── resource_benchmarks.rs   # Resource access
│   └── concurrent_benchmarks.rs # Concurrency stress tests
├── benchmark_results/           # Historical data
│   ├── baseline_v0.1.json      # Current shitty performance
│   └── reports/                # HTML reports
└── scripts/
    ├── bench.sh                # Run benchmarks
    └── compare.sh              # Compare results
```

## Core Benchmarks to Write

### 1. Model Operations (`model_benchmarks.rs`)

- [ ] **Direct Model Access**
  ```rust
  // Benchmark read performance
  fn benchmark_model_read(c: &mut Criterion) {
      let syzygy = create_test_syzygy();
      c.bench_function("model_read", |b| {
          b.iter(|| {
              black_box(syzygy.model().some_field)
          })
      });
  }
  ```

- [ ] **Model Updates**
  ```rust
  // How fast can we mutate state?
  fn benchmark_model_update(c: &mut Criterion) {
      let mut syzygy = create_test_syzygy();
      c.bench_function("model_update", |b| {
          b.iter(|| {
              syzygy.update(|m| {
                  m.counter += 1;
              })
          })
      });
  }
  ```

- [ ] **Snapshot Creation**
  ```rust
  // This is probably slow as fuck
  fn benchmark_snapshot_creation(c: &mut Criterion) {
      let syzygy = create_test_syzygy_with_big_model();
      c.bench_function("snapshot_creation", |b| {
          b.iter(|| {
              black_box(syzygy.create_snapshot())
          })
      });
  }
  ```

### 2. Effect Dispatch (`dispatch_benchmarks.rs`)

- [ ] **Single Effect Dispatch**
  ```rust
  fn benchmark_single_dispatch(c: &mut Criterion) {
      let mut syzygy = create_test_syzygy();
      c.bench_function("single_effect_dispatch", |b| {
          b.iter(|| {
              syzygy.dispatch(|ctx| {
                  // Minimal work
                  black_box(ctx.model().counter);
              });
              syzygy.handle_effects();
          })
      });
  }
  ```

- [ ] **Batch Effect Processing**
  ```rust
  fn benchmark_batch_effects(c: &mut Criterion) {
      let mut syzygy = create_test_syzygy();
      
      c.bench_function("batch_1000_effects", |b| {
          b.iter(|| {
              // Queue up effects
              for _ in 0..1000 {
                  syzygy.dispatch(|ctx| {
                      ctx.model_mut().counter += 1;
                  });
              }
              // Process them all
              syzygy.handle_effects();
          })
      });
  }
  ```

- [ ] **Async Task Spawning**
  ```rust
  fn benchmark_async_task_spawn(c: &mut Criterion) {
      let runtime = tokio::runtime::Runtime::new().unwrap();
      let mut syzygy = create_test_syzygy();
      
      c.bench_function("async_task_spawn", |b| {
          b.to_async(&runtime).iter(|| async {
              syzygy.task(|ctx| async move {
                  // Simulate async work
                  tokio::time::sleep(Duration::from_nanos(1)).await;
              });
          })
      });
  }
  ```

### 3. Resource Access (`resource_benchmarks.rs`)

- [ ] **Resource Retrieval (Current Cloning Disaster)**
  ```rust
  fn benchmark_resource_get(c: &mut Criterion) {
      let syzygy = create_syzygy_with_resources();
      
      c.bench_function("resource_get_clone", |b| {
          b.iter(|| {
              let resource: TestResource = syzygy.resource();
              black_box(resource);
          })
      });
  }
  ```

- [ ] **Resource Type Resolution**
  ```rust
  fn benchmark_resource_type_lookup(c: &mut Criterion) {
      let syzygy = create_syzygy_with_many_resources(); // 50+ types
      
      c.bench_function("resource_type_lookup", |b| {
          b.iter(|| {
              let _: Resource50 = syzygy.resource();
          })
      });
  }
  ```

- [ ] **Concurrent Resource Access**
  ```rust
  fn benchmark_concurrent_resource_access(c: &mut Criterion) {
      use std::sync::Arc;
      use std::thread;
      
      let syzygy = Arc::new(create_syzygy_with_resources());
      
      c.bench_function("concurrent_resource_access_4_threads", |b| {
          b.iter(|| {
              let handles: Vec<_> = (0..4).map(|_| {
                  let syzygy = Arc::clone(&syzygy);
                  thread::spawn(move || {
                      for _ in 0..100 {
                          let _: TestResource = syzygy.resource();
                      }
                  })
              }).collect();
              
              for h in handles {
                  h.join().unwrap();
              }
          })
      });
  }
  ```

### 4. Concurrent Operations (`concurrent_benchmarks.rs`)

- [ ] **Multi-threaded Effect Dispatch**
- [ ] **Lock Contention Measurements**
- [ ] **AsyncContext Creation Under Load**
- [ ] **Channel Throughput Limits**

## Benchmark Scenarios

### Real-World Usage Patterns

- [ ] **Game Loop Simulation**
  ```rust
  // 60fps game updating multiple entities
  fn benchmark_game_loop_60fps(c: &mut Criterion) {
      let mut syzygy = create_game_state();
      
      c.bench_function("game_loop_60fps_1000_entities", |b| {
          b.iter(|| {
              // Simulate one frame
              for entity_id in 0..1000 {
                  syzygy.dispatch(move |ctx| {
                      update_entity(ctx, entity_id);
                  });
              }
              syzygy.handle_effects();
          })
      });
  }
  ```

- [ ] **Web Server State Management**
  ```rust
  // Concurrent reads with occasional writes
  fn benchmark_web_server_pattern(c: &mut Criterion) {
      // 95% reads, 5% writes
      // Measure latency percentiles
  }
  ```

- [ ] **Event Sourcing Pattern**
  ```rust
  // Many small updates building up state
  fn benchmark_event_sourcing(c: &mut Criterion) {
      // Apply 10k events and measure throughput
  }
  ```

## Measurement Guidelines

### What to Measure

- [ ] **Throughput**: Operations per second
- [ ] **Latency**: Time per operation (with percentiles)
- [ ] **Memory**: Allocations per operation
- [ ] **Scalability**: Performance vs thread count
- [ ] **Overhead**: Empty operation baseline

### How to Measure Properly

1. **Warm-up is Critical**
   ```rust
   c.bench_function("important_operation", |b| {
       b.warm_up_time(Duration::from_secs(3)); // Let caches warm up
       b.measurement_time(Duration::from_secs(10)); // Measure longer
       b.iter(|| { /* ... */ })
   });
   ```

2. **Use Black Box**
   ```rust
   // Prevent compiler from optimizing away your code
   let result = expensive_operation();
   black_box(result); // Force compiler to keep this
   ```

3. **Measure What Users Do**
   - Don't benchmark implementation details
   - Benchmark public API usage patterns
   - Include setup/teardown if users would do it

4. **Statistical Significance**
   ```rust
   // Configure Criterion for reliability
   criterion_group! {
       name = benches;
       config = Criterion::default()
           .significance_level(0.1)
           .sample_size(200)
           .measurement_time(Duration::from_secs(15));
       targets = benchmark_function
   }
   ```

## Storing and Tracking Results

### Baseline Storage

- [ ] **Initial Baseline**
  ```bash
  # Run and save baseline
  cargo bench --bench syzygy_benchmarks -- --save-baseline pre-unfuck
  ```

- [ ] **Per-Phase Baselines**
  - `phase1-post-unsafe-removal`
  - `phase2-post-simplification`
  - `phase3-post-features`
  - `final-1.0`

### Continuous Tracking

- [ ] **Git Hooks**
  ```bash
  #!/bin/bash
  # pre-push hook
  cargo bench --bench syzygy_benchmarks -- --baseline main
  if [ $? -ne 0 ]; then
      echo "Performance regression detected!"
      exit 1
  fi
  ```

- [ ] **CI Integration**
  ```yaml
  # .github/workflows/bench.yml
  benchmark:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - run: cargo bench -- --save-baseline pr-${{ github.event.number }}
      - uses: actions/upload-artifact@v2
        with:
          name: benchmarks
          path: target/criterion
  ```

- [ ] **Performance Tracking Database**
  ```sql
  -- Simple SQLite schema
  CREATE TABLE benchmarks (
      id INTEGER PRIMARY KEY,
      commit_hash TEXT NOT NULL,
      benchmark_name TEXT NOT NULL,
      mean_time_ns INTEGER NOT NULL,
      stddev_ns INTEGER NOT NULL,
      timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
  );
  ```

### Visualization and Reports

- [ ] **HTML Reports**
  - Criterion generates these automatically
  - Store in `benchmark_results/reports/`
  - Link from README

- [ ] **Comparison Scripts**
  ```bash
  #!/bin/bash
  # compare.sh
  cargo bench -- --baseline main --color always | tee comparison.txt
  # Parse and create markdown report
  ```

- [ ] **Performance Dashboard**
  - Use gnuplot or Python for trends
  - Show performance over time
  - Highlight regressions

## Anti-Patterns to Avoid

### Don't Do This Shit

1. **Micro-benchmarking Useless Things**
   ```rust
   // WRONG: Nobody gives a fuck about this
   fn benchmark_option_is_some(c: &mut Criterion) {
       let opt = Some(42);
       c.bench_function("option_is_some", |b| {
           b.iter(|| opt.is_some())
       });
   }
   ```

2. **Benchmarking Debug Builds**
   ```bash
   # WRONG - Always use release mode
   cargo bench  # This uses release by default, good
   ```

3. **Ignoring Variance**
   - If stddev > 10% of mean, your benchmark is shit
   - Fix the variance before trusting results

4. **Optimizing Before Measuring**
   - Measure first, optimize second
   - Your intuition about performance is usually wrong

## Action Items

### Immediate Tasks

- [ ] Create `benches/` directory structure
- [ ] Write baseline benchmarks for current API
- [ ] Run benchmarks and save v0.1 baseline
- [ ] Set up CI to run benchmarks on PRs
- [ ] Document current performance numbers

### For Each Unfucking Phase

- [ ] Run benchmarks before changes
- [ ] Make changes
- [ ] Run benchmarks after changes
- [ ] Document improvements/regressions
- [ ] Only merge if performance is acceptable

### Benchmark Review Checklist

- [ ] Does it measure real usage patterns?
- [ ] Is the measurement time sufficient?
- [ ] Are we using black_box properly?
- [ ] Is variance under control?
- [ ] Did we warmup properly?
- [ ] Are results reproducible?

## Scripts to Create

### `bench.sh`
```bash
#!/bin/bash
# Run all benchmarks and save results
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
cargo bench --bench syzygy_benchmarks -- --save-baseline $TIMESTAMP
echo "Results saved to baseline: $TIMESTAMP"
```

### `bench-compare.sh`
```bash
#!/bin/bash
# Compare current with baseline
BASELINE=${1:-main}
cargo bench --bench syzygy_benchmarks -- --baseline $BASELINE
```

### `bench-watch.sh`
```bash
#!/bin/bash
# Watch for changes and re-run benchmarks
cargo watch -x 'bench --bench syzygy_benchmarks'
```

## Remember

- Benchmarks are code too - keep them clean and maintainable
- If a benchmark takes > 30s to run, it's too long
- Measure what matters to users, not your ego
- Performance without correctness is worthless
- Sometimes "fast enough" is actually fast enough

Now stop procrastinating and write some fucking benchmarks. You can't optimize what you can't measure, and right now you're flying blind.