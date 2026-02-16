# Decisions

## Scope

- **Single root model**: Users compose their own app state struct; no internal model registry or slot system. Decided early to avoid the complexity of dynamic model storage.
- **Clone-per-effect resources**: Shell clones `Resources` for each effect invocation. Users wrap heavy deps in `Arc<_>`. Simple, predictable, no lifetime gymnastics.

## Technology

- **crossbeam-channel for Core events**: Chosen over `tokio::sync::mpsc` to keep Core runtime-agnostic. Core never touches async.
- **SmallVec<[_; 4]> for Command steps**: Most commands have 0-4 steps. Avoids heap allocation for the common case.
- **TypeId-based executor registry**: Executors keyed by concrete type. Avoids trait-object dispatch on the registration path while keeping the spawn path flexible.

## Implementation

- **No `--all-features` gate**: The `rayon` feature flag is broken (never declared in `[features]`). Gate runs default features only until fixed.

## Open Questions

- **Batch vs Parallel**: Currently handled identically in Shell. Should Parallel actually dispatch concurrently, or should the distinction be removed?
- **`process_events` allocation**: Returns a new `Vec` every tick. Conflicts with "zero-overhead" claim. Needs API decision (callback? reusable buffer? drain iterator?).
- **Dead error types**: `EffectError`, `CommandError`, several `ShellError` variants are never constructed. Delete or find a use?
- **`ResourceCell`**: Exported in prelude, used by nobody. Keep or delete?
