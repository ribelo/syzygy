# Syzygy De-Bloat Request

You are an autonomous engineering agent with deep Rust expertise. Inspect the `syzygy` repository from first principles to isolate the minimal surface required to deliver deterministic, Elm-style state management for event-driven systems.

## Core Objectives
- Map the essential invariants that must hold for safety: ordered event processing, explicit command execution, predictable shutdown, bounded channels, and transparent executor lifecycle management.
- Identify every module, feature flag, and dependency that does *not* contribute directly to those invariants. Quantify what each piece costs (complexity, binary size, dependencies, runtime overhead).
- Propose the smallest coherent architecture that preserves the invariants while removing or simplifying non-essential code paths.

## Analysis Requirements
- Work feature-by-feature (`shell`, executors, presets, CLI helpers, testing-only flags) and justify whether each survives the cut. Prefer collapsing optional combinations over introducing new abstraction layers.
- Highlight redundant traits, wrappers, or async indirections that can be collapsed into simpler, explicit implementations.
- Examine benchmarks, examples, and helper utilities; flag any that should migrate to docs/out-of-tree crates if not required for the core runtime.
- Treat safety first, performance second, developer convenience third. Do not sacrifice determinism, resource hygiene, or error handling.

## Expected Output
1. Concise list of non-negotiable invariants and the files enforcing them.
2. Inventory of suspected bloat with rationale, estimated risk, and suggested remediation (delete, merge, replace with simpler pattern).
3. Migration plan for applying the lean architecture, including the minimal test suite needed to prove nothing critical regressed.

Keep the tone surgical and objective. Avoid marketing copy and avoid suggesting changes to documentation unless a code deletion makes a doc misleading.
