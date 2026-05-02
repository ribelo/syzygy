# Repository Guidelines

## Issue Tracking (bd)

- Significant work (features, refactors, bugs) requires a `bd` issue.
- Trivial fixes (typos, formatting, minor doc tweaks) do not require an issue.
- `bd` is the single source of truth for tasks, epics, and bugs.
- Do not use markdown TODO lists or external trackers.

### Task Sizing

- **Tasks** should be completable in a few hours of focused work.
- **Epics** can be large and grow over time; add tasks to epics as work progresses.
- Avoid "draw the rest of the owl" tasks. If a task is too big, split it.
- Consider session interruption risk: work that's hard to resume should be a smaller task.

### Definition of Ready (DoR)

Before an issue is set to `in_progress`:

1. **Value Identified**: Clear statement of problem and desired outcome.
2. **No Blockers**: External dependencies and context resolved.
3. **Boundary Sketched**: Technical approach understood.
4. **DoD Defined**: Specific, measurable acceptance criteria listed.

### Definition of Done (DoD)

Work is only "Done" (closed) when:

1. **Implementation** follows project patterns.
2. **Validated**: Tests pass, quality gate green.
3. **Zero Waste**: No TODO stubs or dead code.
4. **Truthful Docs**: Documentation matches behavior.
5. **Realized**: Committed, issue updated, pushed to remote.

### Saying "Done" (chat / handoff rule)

Before the agent says "I'm done" (or hands off work as finished):

1. The agent MUST run the project quality gate.
2. The agent MUST NOT present broken / failing code as finished work.
3. If the quality gate fails:
   - If caused by the agent's changes: fix it.
   - If pre-existing or unrelated: stop and ask, record as bd issue.

### Handling Discoveries

When implementing task A reveals need for task B:

1. Stop work on A if B is a blocker.
2. Create task B with appropriate priority.
3. Mark B as `in_progress`.
4. Complete B first, then resume A.

## Workflow Phases

### Planning Phase

- **Low autonomy**: Ask frequently before decisions.
- Research prior art; some ideas have been tried and failed.
- Update `docs/qa.md` after any decision, even implicit ones.

### Implementation Phase

- **High autonomy**: Follow DoD, proceed without asking for small decisions.
- Ask before implementing unspecified cases.
- Fix obvious gate failures; ask about unclear ones.

## Quality Gate

Single command that must pass before claiming work is done or pushing:

```bash
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
```

Note: `--all-features` is currently broken (see known issues). Do not use until fixed.

---

## 1) Safety Rules

- **Simple control flow only:** no `goto`, recursion, or equivalents. Maintain an acyclic call graph for provable boundedness.
- **Bound every loop:** Provide static justification for upper bounds. Enforce hard limits if iteration counts are uncertain.
- **No dynamic allocation after initialization** in critical/long-running paths; design memory upfront with arenas/pools/fixed capacities.
- **Small functions:** keep below ~60 logical lines. Larger functions often signal unclear structure.
- **Average 2+ side-effect-free assertions per function; trigger explicit recovery on failure.**
- **Declare variables in narrowest possible scope. Don't reuse variables for multiple purposes.**
- **Always check results, validate inputs, and propagate errors explicitly. Never ignore results without clear justification.**
- **Limit metaprogramming and macros. Avoid complex conditional compilation or macro-based DSLs.**
- **Avoid raw pointers/function-style indirection on critical paths; prefer static dispatch/generics.**
- **Zero warnings, continuous strict static analysis. Rewrite code for clarity if analysis/tools incorrectly warn.**

> **Favor strict, safety-conscious guidelines over idiomatic shortcuts that sacrifice resilience.**

---

## 2) Extensions

- **Prioritize:** Safety → Performance → Developer Experience.
- **Pair assertions at caller/callee to enforce contracts; check required/forbidden behaviors at compile time when possible.**
- **Centralize control/state at parent; keep leaf functions pure; batch ops to amortize cost.**
- **Sketch performance: estimate bandwidth/latency for network, disk, memory, CPU. Optimize the slowest/highest-frequency resource.**
- **Buffer/batch to bound work; don't react immediately to interrupts.**
- **Prefer explicit call-site options over defaults to avoid subtle behavior.**
- **Favor explicitly sized types (e.g., `u32`) for protocols/storage over arch-dependent ones.**

---

## 3) Pragmatism and Simplicity

- **Avoid unnecessary features/abstraction; pursue the 80/20 solution.**
- **Abstractions must earn their keep-prefer boring, simple solutions.**
- **Delete unjustified complexity and legacy shims. Ship the simplest design.**
- **Respect data/layout/hardware constraints. Minimize indirection and do IO at boundaries.**
- **Favor referentially transparent/pure functions over hidden state or mutation.**
- **If confused, stop and fix (Kill FOLD).**
- **Prototype minimal demos when stuck to expose real constraints.**
- **Invest in robust, human-friendly logging; debugging is a major time expense.**
- **Use generics moderately; avoid over-complex type-level tricks.**

---

## 4) Language-Specific Conventions

- **Only use unsafe when necessary; isolate and document invariants/assertions/tests in minimal modules.**
- **Prefer borrowing to cloning; limit ownership/lifetime complexity.**
- **Pre-allocate buffers, arrays, slabs, arenas; avoid heap growth post-init on critical paths.**
- **Default to static dispatch; restrict trait objects on hot paths unless strictly needed and justified.**
- **Keep macros trivial; don't stack `cfg` attributes/explode test matrix.**
- **Idiomatic, explicit names with unit suffixes for clarity (e.g., `_ms`).**

---

## 5) Testing, Verification, and Observability

- **Test negative as rigorously as positive cases: model boundaries, use property/fuzz testing to find bugs.**
- **Fix bugs by first writing failing regression tests.**
- **Handle all errors. Never ignore Results. Use structured, context-rich logs for debugging.**
- **After edits or tests, validate outcomes and document next steps; self-correct if validation fails.**

---

## 6) Communication & Collaboration

- **Ask clarifying questions if any aspect is unclear or ambiguous before proceeding.**
- **Propose or advise simpler, more maintainable solutions when requested approach is overly complex; reject unnecessary complexity.**
- **Use external resources instead of reinventing solutions when unfamiliar requirements arise.**

---

## 7) PR Format & Pre-Merge Checklist

- **PR template:** Reasoning → Decision → Plan (tests/telemetry) → Result.
- **Checklist:**
  - [ ] Bounds everywhere
  - [ ] ≥2 assertions/function
  - [ ] ≤60 lines/function
  - [ ] Zero warnings
  - [ ] Static analysis clean
  - [ ] Negative tests present
  - [ ] Actionable logs
- **Conventional prefixes (`feat:`, `fix:`, `docs:`, `chore:`); single-purpose commits.**
 - **PR hygiene:** Link relevant TODO items, include validation commands, and record follow-ups in TODO instead of the PR thread.

---

## Project Structure & Module Organization
`src/lib.rs` exposes the library surface; sibling modules (`core.rs`, `shell.rs`, `builder.rs`, `executor/`, etc.) hold the Elm-inspired runtime pieces. Integration tests sit under `tests/`, Criterion benchmarks under `benches/`, and runnable reference apps in `examples/`. High-level design notes live in `docs/`.

## Build, Test, and Development Commands
- `cargo check` - quick compilation guard while iterating.
- `cargo build --all-targets` - compiles library, examples, benches, and tests together.
- `cargo fmt --all --check` - verify formatting without writing.
- `cargo clippy --all-targets -- -D warnings` - enforce clippy pedantic + custom warn set from `Cargo.toml`.
- `cargo test` - runs unit and integration suites with default (`shell`, `rt-compio`) features.
- `cargo run --example 01_basic_counter` - smoke test the async workflow; swap the example name as needed.
- `./scripts/check-feature-matrix.sh` - compile/check supported feature combinations and downstream fixture expectations.
- **Gate** (run before every commit): `cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test`

## Coding Style & Naming Conventions
Use Rust 2021 defaults: 4-space indentation, `snake_case` for functions and modules, `UpperCamelCase` types, and `SCREAMING_SNAKE_CASE` constants. Avoid `dbg!`, `todo!`, and unchecked `.unwrap()` calls, which clippy flags at `warn`. Prefer explicit clones over implicit copies, keep modules cohesive, and follow the existing `mod.rs` entry-point pattern when splitting subsystems.

## Testing Guidelines
Keep scenario-driven checks in `tests/` and target property-heavy logic with `proptest`. Name files after the behaviour under scrutiny (e.g., `tests/core_shutdown.rs`). Run `cargo test` plus `./scripts/check-feature-matrix.sh` when touching feature/runtime boundaries. Doctest snippets belong in `docs/` only when they compile against the public API.

## Commit Guidelines

### When to Commit

- **Coherent feature slice**: Commit when a logical unit works.
- **Full task**: For smaller tasks, when complete.
- **WIP**: Acceptable if marked and tracked.

### Commit Style

- Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`, `refactor:`, `test:`
- Imperative, present-tense subjects ("Guard builder tests requiring InlineAsync").
- Run quality gate before committing.
- Never use `git add -A`.
- Single-purpose commits, easy to revert.

### Branching

- Single branch (master/main) for all work.
- Feature branches only when explicitly requested.

### Push Rules

- Never push to shared branches with failing tests without user approval.
- WIP/failing tests can go to feature branches.

## Benchmarks & Performance Checks
Use `cargo bench --bench command_performance` or `cargo bench --bench shell_throughput` before merging performance-sensitive work. Note regressions in the PR description, including reproduction commands and environmental quirks.

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   bd sync
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds
