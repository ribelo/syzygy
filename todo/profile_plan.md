# Profiling Plan – Measuring Static vs Dynamic Dispatch

This guide is **turn-key**: follow every step in order and you will
produce numbers and flame-graphs that explain the performance
difference. No prior profiling experience required.

---

## 0. Prerequisites

| Tool | Install command (run **once**) |
|------|--------------------------------|
| **Rust stable** | already installed (project compiles) |
| **criterion** | Comes from `[dev-dependencies]`, no action |
| **flamegraph** | `cargo install flamegraph cargo-flamegraph` |
| **perf** (Linux) | `sudo apt install linux-tools-common` |
| **dhat-rs** (heap profiler) | `cargo install dhat` |

> ⚠️ All commands are executed from the repository root  
> (`syzygy` is the crate directory).

---

## 1. Set Up Two Branches

```
git switch static-dispatch-implementation   # static implementation
git switch -c bench-static                  # optional throw-away branch for profiling
git switch dynamic-dispatch-baseline        # previous dynamic dispatch
```

If you have **untracked or modified files**, stash them first so `git switch`
doesn’t fail:

```
git stash -u          # saves untracked + tracked edits
```

Make sure **both** branches compile:

```
cargo check
```

---

## 2. Criterion Baseline Runs

### 2.1 Record Static Baseline

```
git switch bench-static
cargo clean
cargo bench --bench minimal_profile --bench simple_profile \
            -- --save-baseline static
```

### 2.2 Record Dynamic Baseline

```
git switch dynamic-dispatch-baseline
cargo clean
cargo bench --bench minimal_profile --bench simple_profile \
            -- --save-baseline dynamic
```

### 2.3 Compare Baselines

```
cargo bench --bench minimal_profile --bench simple_profile \
            -- --baseline static --baseline dynamic
```

Criterion prints **% faster/slower** per benchmark.  
Open the HTML report if you prefer:

```
xdg-open target/criterion/report/index.html   # Linux
```

---

## 3. CPU Flamegraphs

### 3.1 Static

```
git switch bench-static
cargo flamegraph --bench simple_profile --root --open
```

### 3.2 Dynamic

```
git switch dynamic-dispatch-baseline
cargo flamegraph --bench simple_profile --root --open
```

Browser tab opens automatically.  
Look for dominating functions (`crossbeam_channel::*` vs v-table call).

---

## 4. Perf Counters

```
git switch bench-static
perf stat -d target/release/benches/simple_profile empty_dispatch_handle
```

Repeat on `dispatch-dyn`.  
Compare:

* `cache-misses`
* `branch-misses`
* `instructions`

More misses on **static** = channel overhead.

---

## 5. Heap-Allocation Check (optional)

```
git switch bench-static
cargo dhat --release --bench simple_profile -- --filters empty_dispatch_handle
# Opens viewer; look for crossbeam allocations
```

Repeat on `dispatch-dyn`.

---

## 6. Type Size Sanity

Create a quick playground test (scratch file):

```rust
println!("Msg size      = {}", std::mem::size_of::<syzygy::dispatch::Msg<YourEffect>>());
println!("Box<dyn Eff>  = {}", std::mem::size_of::<Box<dyn std::any::Any>>());
```

Run:

```
cargo run --quiet
```

Expect `Msg` > `Box`.

---

## 7. Micro-bench **Box\<dyn>** vs Generic Enum

The original code never had a `VecDeque` queue; the real comparison is
between **boxed dynamic calls** and **monomorphised enum calls**.

Create `benches/boxed_vs_enum.rs`:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

#[inline(always)]
fn boxed_call(f: Box<dyn FnOnce()>) {
    (f)();               // v-table dispatch
}

#[inline(always)]
fn enum_call<F: FnOnce()>(f: F) {
    f();                 // static dispatch
}

fn bench_box(c: &mut Criterion) {
    c.bench_function("box_dyn", |b| {
        b.iter(|| {
            boxed_call(Box::new(|| black_box(())));
        });
    });
}

fn bench_enum(c: &mut Criterion) {
    c.bench_function("enum_generic", |b| {
        b.iter(|| {
            enum_call(|| black_box(()));
        });
    });
}

criterion_group!(benches, bench_box, bench_enum);
criterion_main!(benches);
```

Run the bench on **both** branches:

```
cargo bench --bench boxed_vs_enum
```

If the timings for `box_dyn` and `enum_generic` are nearly identical,
then dynamic dispatch itself is **not** the bottleneck. Any remaining
slow-down must therefore come from the transport layer (crossbeam
channel) introduced in the static branch.

---

## 8. (Optional) Continuous Integration Guard

Add a GitHub Actions job that:

1. Runs `cargo bench -- --save-baseline main`.
2. Compares PR results against `main`.
3. Fails if regression > 10 %.

See Criterion docs:  
<https://bheisler.github.io/criterion.rs/book/user_guide/continuous_integration.html>

---

## Finished

You now have:

* Quantitative slowdown data (Criterion).
* Call-graphs showing where time is spent (Flamegraph).
* CPU-counter evidence (perf).
* Allocation evidence (dhat).
* Micro-bench confirmation that dynamic (v-table) dispatch cost is negligible.

Use these artefacts to justify API or implementation changes.