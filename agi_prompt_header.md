AGI Prompt: Syzygy Architecture Review and Code Audit

Role
- Act as a principal Rust architect and staff engineer.
- Be rigorous, pragmatic, and opinionated where tradeoffs matter.
- Optimize for correctness, simplicity, determinism, and performance.

Mission
- Provide a deep architecture analysis of the Syzygy library, focusing on design, boundaries, and runtime model.
- Produce a thorough code review: safety, performance, readability, API ergonomics, and testability.
- Deliver actionable improvements with a prioritized roadmap and low-risk refactors.

Repository Context
- Use the companion repository digest file `SYZYGY_REPO_DIGEST.md` (generated via repomix). Use it for file references and line navigation.

What To Analyze
- Architecture and Design
  - Core/Shell split (pure vs impure) and Command/Task semantics
  - Executor abstraction: async/blocking/resource-blocking; registry design and trait boundaries
  - Event processing model: ordering guarantees, determinism, backpressure, queue sizing
  - Error-as-events pattern: clarity, composability, and failure isolation
  - Resource management and cloning strategy; ergonomics for heavy dependencies
  - Public API surface and builder ergonomics; discoverability and DX
  - Concurrency model across executors; safety, cancellation, shutdown semantics
  - Extensibility: adding custom executors, effect kinds, and integrations

- Code Quality and Correctness
  - Safety and correctness (Send/Sync boundaries, lifetimes, ownership)
  - Performance hotspots and allocation patterns; avoid needless clones
  - Error handling: error types, propagation, and diagnosability
  - Naming, cohesion, and module structure; crate layout consistency
  - Testing strategy: unit, property, and integration coverage; determinism
  - Docs, comments, examples, and architecture explanation accuracy

Deliverables
1) Architecture Review
   - High-level mental model and dataflow (text diagram ok)
   - Strengths and tradeoffs; gaps/risks with concrete examples
   - Comparison to alternatives (TEA, ECS, actor models) where helpful

2) Code Review Findings
   - File-by-file highlights referencing `path:line` (e.g., `src/core.rs:120`)
   - Categories: correctness, safety, perf, API design, ergonomics
   - Quick wins vs. deeper refactors

3) Improvements & Roadmap
   - Prioritized list (high → medium → low), with rationale and expected impact
   - Suggested refactor sketches (small, incremental, low-risk first)
   - Testing plan: specific tests to add, where, and why

4) Performance Guidance
   - Hypotheses on bottlenecks and contention points
   - Microbench ideas and methodology (what to measure, expected outcome)
   - Executor policy suggestions (when to use which, defaults)

5) API/DX Recommendations
   - Ergonomic tweaks to the builder and key types
   - Documentation improvements and example coverage

Output Format
- Use clear sections matching Deliverables above.
- When citing code, use repository-relative file paths with optional line numbers: `path:line`.
- Prefer concise bullets; include short code snippets or patch diff hunks where it clarifies.
- End with a short “Top 5 Actions” list that can be executed immediately.

Assumptions & Constraints
- Favor stable Rust and widely used crates.
- Keep public API changes source-compatible when feasible; note breaking changes explicitly.
- Maintain deterministic behavior and FIFO guarantees as core tenets.

Now read the repository digest below and produce your review.
