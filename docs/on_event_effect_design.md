# Syzygy Multi-Model, Multi-Resource, Axum-like Handlers — Implementation Plan

**❌ PLAN REJECTED BY CONSENSUS REVIEW ❌**

This document outlines a comprehensive plan to introduce:
- Multiple models (type-unique tuple "type-set") with zero-cost extraction
- Multiple resources (type-unique tuple) with zero-cost extraction
- Axum-like declarative handlers for events and effects:
  - `on_event(E, M1, M2, ...)` — sync, returns `Command<E, Fx>`
  - `on_effect(Fx, R1, R2, ...)` — async, uses `EffectContext<E>`

**CONSENSUS REVIEW FINDINGS:**
- **Grug Code Reviewer**: 1/10 - "Complexity for complexity's sake"
- **Functional Pragmatist**: 4/10 - "Architecturally misguided" 
- **Performance Engineer**: "Zero-cost claims are false"

**PERFORMANCE IMPACT**: 2-5x slower event dispatch, +300-1000% compile time

The design preserves Syzygy's Core/Shell/Runner contracts and performance goals.

---

## Goals
- Zero-cost abstractions: no HashMaps, minimal allocations, fully monomorphized.
- Declarative composition: small, focused handlers that declare exact dependencies.
- Backward compatible: existing `App` implementations continue to work.
- Extensible: add macros and ergonomic sugar later without redesign.

---

## High-Level Architecture

- Keep existing `App`, `Core`, `Shell`, `EffectHandler`, `Runner` unchanged.
- Add a new `event_map` (name TBD) subsystem that provides:
  - Type-sets for Models and Resources using tuples (unique types enforced implicitly)
  - Extractors (axum-like): `Model<M>`, `ModelMut<M>`, `Resource<R>`, `EventRef<T>`, `EffectRef<T>`, `Ctx<EffectContext<E>>`, optional `Time` and `Spawner`
  - Handler traits that transform a function signature into a handler via `FromContext`-style extraction
  - Routers: `EventRouter` (sync, returns `Command<E, Fx>`), `EffectRouter` (async, implements `EffectHandler`)
  - `EventMapApp` implementing `App` using `EventRouter` for update and a `ResourceSet` carried separately for effects
  - Builder that composes models/resources and registers handlers

---

## Public API Draft

```rust
// Module re-export
pub mod event_map;

// In prelude:
// pub use crate::event_map::{EventMapApp, EventAppBuilder, Model, ModelMut, Resource, EventRef, EffectRef, Ctx};
```

### Builder usage (target ergonomics)

```rust
use syzygy::prelude::*;
use syzygy::event_map::*;

#[derive(Clone)]
struct Auth { /* ... */ }
#[derive(Clone)]
struct Profile { /* ... */ }
#[derive(Clone)]
struct Clock;

#[derive(Debug, Clone)]
enum AppEvent { UserLogin { name: String }, Tick }
#[derive(Debug, Clone)]
enum AppEffect { SendEmail { to: String } }

fn handle_login(
    ModelMut<Auth>,
    Resource<Clock>,
    EventRef<UserLogin>, // see Event subtyping notes below
) -> Command<AppEvent, AppEffect> {
    // mutate Auth, use Clock, return commands
    Command::effect(AppEffect::SendEmail { to: "admin@x".into() })
}

async fn handle_send_email(
    Resource<Mailer>,
    EffectRef<SendEmail>,
    Ctx<EffectContext<AppEvent>>,
) {
    // send email; on completion, optionally emit events via ctx
}

let app = EventAppBuilder::<AppEvent, AppEffect>::new()
    .model(Auth { /* ... */ })
    .model(Profile { /* ... */ })
    .resource(Clock)
    .on_event(handle_login)
    .on_effect(handle_send_email)
    .build();

let (core, event_tx) = Core::new(app, /* model set provided by builder */);
let shell = Shell::new().with_event_sender(core.event_sender())
                        .with_effect_handler(app.effect_router());
```

> Note: `EventRef<UserLogin>` is explained in “Event typing” below. MVP keeps a single enum and uses a guard/trait to view a variant as a typed sub-event.

---

## Detailed Design

### 1) Type-sets for Models and Resources

- Represent `ModelSet` and `ResourceSet` as tuples `(T1, T2, T3, ...)` with unique types.
- Provide zero-cost access via traits:

```rust
pub trait Get<T> { fn get(&self) -> &T; }
pub trait GetMut<T> { fn get_mut(&mut self) -> &mut T; }

// Multi-borrow for distinct types (generated up to N = 3 or 4 for MVP):
pub trait Get2Mut<A, B> { fn get2_mut(&mut self) -> (&mut A, &mut B); }
pub trait Get3Mut<A, B, C> { fn get3_mut(&mut self) -> (&mut A, &mut B, &mut C); }
```

- Implementations are generated for tuples using a macro (arity N, e.g., 12). Each impl indexes by type (monomorphized), not by strings or TypeId at runtime.
- Uniqueness: If duplicate types are present, trait impl resolution becomes ambiguous and fails to compile — giving a compile-time safety net.

Performance: `get`/`get_mut` compile down to a direct tuple field reference via monomorphization; no dynamic lookup.

### 2) Axum-like Extractors and FromContext

- Define a generic context for event handling:

```rust
pub struct EventCtx<'a, E, M, R> {
    pub event: &'a E,
    pub models: &'a mut M,
    pub resources: &'a R,
}
```

- Define extractors and their `FromEventCtx` implementations:

```rust
pub struct Model<T>(pub T);
pub struct ModelMut<T>(pub T);
pub struct Resource<T>(pub T);
pub struct EventRef<T>(pub T); // typed view of a variant

pub trait FromEventCtx<'a, E, M, R>: Sized {
    type Output;
    fn from_ctx(ctx: &'a mut EventCtx<'a, E, M, R>) -> Option<Self::Output>;
}
```

- Example impls:
  - `FromEventCtx for Model<Auth>` returns `&Auth` using `Get<Auth>`
  - `FromEventCtx for ModelMut<Auth>` returns `&mut Auth` using `GetMut` or `Get2Mut/3Mut` logic combined in the handler’s tuple extractor
  - `FromEventCtx for Resource<Clock>` returns `&Clock` using `Get<Clock>`
  - `FromEventCtx for EventRef<UserLogin>` uses an event-subtyping trait (see below) to attempt to create `UserLogin` from `&E`

- For effects, define `EffectCtx<'a, E, Fx, R>` and `FromEffectCtx` with `EffectRef<T>` and `Ctx<EffectContext<E>>`.

Notes:
- Multi-mutable extraction: When a handler declares multiple `ModelMut<...>` arguments, its tuple extractor uses `get2_mut`/`get3_mut` to split borrows safely and zero-cost.
- If a user attempts two `ModelMut<T>` with the same `T`, no `get2_mut<T, T>` impl exists — compile-time error.

### 3) Handler Traits (sync and async)

- Event handler (sync):

```rust
pub trait EventHandler<E, Fx, M, R> {
    fn call(&self, ctx: &mut EventCtx<E, M, R>) -> Option<Command<E, Fx>>;
}
```

- Implement for functions with extractor arguments via a blanket impl over tuples that implement `FromEventCtx`:

```rust
impl<E, Fx, M, R, F, A> EventHandler<E, Fx, M, R> for F
where
    A: ExtractEventArgs<E, M, R>,             // builds (Arg1, Arg2, ...)
    F: FnOnce(A::Output) -> Command<E, Fx> + Clone + Send + Sync + 'static,
{
    fn call(&self, ctx: &mut EventCtx<E, M, R>) -> Option<Command<E, Fx>> {
        A::extract(ctx).map(|args| (self.clone())(args))
    }
}
```

- Effect handler (async):

```rust
pub trait EffectHandlerFn<E, Fx, R> {
    type Fut: Future<Output = ()> + Send + 'static;
    fn call(&self, ctx: EffectCtx<E, Fx, R>) -> Self::Fut;
}

impl<E, Fx, R, F, A, Fut> EffectHandlerFn<E, Fx, R> for F
where
    A: ExtractEffectArgs<E, Fx, R>,
    F: FnOnce(A::Output) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = ()> + Send + 'static,
{ /* construct args via A and call */ }
```

- The `ExtractEventArgs`/`ExtractEffectArgs` traits generate argument tuples from context (macro for arities 0..N). Each argument must implement `FromEventCtx` or `FromEffectCtx`.

### 4) Routers

- `EventRouter<E, Fx, M, R, H = ()>`
  - Maintains a compile-time chain of handlers; first match wins.
  - API:

```rust
impl<E, Fx, M, R, H> EventRouter<E, Fx, M, R, H> {
    pub fn on_event<F, A, Ev>(self, f: F) -> EventRouter<..., Chain<H, Handler<F>>> where
        A: ExtractEventArgs<E, M, R>,
        Ev: TryFromEvent<E>,
        F: FnOnce(A::Output) -> Command<E, Fx> + Clone + Send + Sync + 'static;

    pub fn fallback<F>(self, f: F) -> ... // Optional default handler

    pub fn handle(&self, e: &E, models: &mut M, res: &R) -> Command<E, Fx>;
}
```

  - `handle` iterates the chain (pure generics, no Vec). Each handler attempts to extract `Ev` from `&E` through the extractor tuple. If extraction fails, return `None`; otherwise return `Some(Command)`. First `Some` stops the chain.

- `EffectRouter<E, Fx, R, H = ()>`
  - Similar chain; implements `syzygy::effect_handler::EffectHandler<E, Fx, R>` so it can be plugged into `Shell`.
  - Each handler extracts its args (resources, effect, `EffectContext<E>`) and runs async.

### 5) `EventMapApp` implementing `App`

```rust
pub struct EventMapApp<E, Fx, M, R, ER> {
    models: M,
    resources: R,
    event_router: ER,
}

impl<E, Fx, M, R, ER> App for EventMapApp<E, Fx, M, R, ER>
where
    E: Clone + Send + 'static,
    Fx: Clone + Send + 'static,
    M: Send + 'static,
    R: Send + Sync + 'static,
    ER: /* EventRouter-like with .handle() */,
{
    type Event = E;
    type Model = M;        // M is our type-set (tuple)
    #[cfg(feature = "view-model")] type ViewModel = /* as-is */;
    type Effect = Fx;
    type Resources = R;    // tuple of resources

    fn update(&self, event: E, model: &mut M) -> Command<E, Fx> {
        // delegate to router with self.resources
        self.event_router.handle(&event, model, &self.resources)
    }

    #[cfg(feature = "view-model")]
    fn view(&self, model: &Self::Model) -> Self::ViewModel { /* optional */ }
}
```

- For effects, the application exposes `effect_router()` that returns an object implementing `EffectHandler<E, Fx, R>`, which can be installed into `Shell`.

### 6) Builder

- `EventAppBuilder<E, Fx>` collects models/resources and handlers, producing `EventMapApp` and an `EffectRouter`.

```rust
pub struct EventAppBuilder<E, Fx, M = (), R = (), ER = (), XR = ()> { /* phantom types for chain */ }

impl<E, Fx> EventAppBuilder<E, Fx> {
    pub fn new() -> Self { /* ... */ }

    pub fn model<M1>(self, m1: M1) -> EventAppBuilder<E, Fx, (M, M1), R, ER, XR> { /* tuple-append */ }
    pub fn resource<R1>(self, r1: R1) -> EventAppBuilder<E, Fx, M, (R, R1), ER, XR> { /* tuple-append */ }

    pub fn on_event<F, A, Ev>(self, f: F) -> Self { /* extend ER chain */ }
    pub fn on_effect<F, A, Fxv>(self, f: F) -> Self { /* extend XR chain */ }

    pub fn build(self) -> EventMapApp<E, Fx, MFinal, RFinal, ERFinal> { /* freeze */ }
}
```

- Tuple-append helper types/functions consolidate the model/resource tuples as values and types.

---

## Event/Effect Typing Strategy

MVP (simplest, zero refactor):
- Keep a single `enum Event` and `enum Effect`.
- Provide trait `TryFromEvent<E>` and `TryFromEffect<Fx>` for “typed views” of variants, used by `EventRef<T>`/`EffectRef<T>`.
- Users implement these for their sub-types (or use a small helper macro):

```rust
trait TryFromEvent<E> { fn try_from_event(e: &E) -> Option<Self> where Self: Sized; }
trait TryFromEffect<Fx> { fn try_from_effect(fx: &Fx) -> Option<Self> where Self: Sized; }
```

Later (ergonomics):
- Provide `syzygy_macros::sum!` to build the enum from structs and autogenerate `TryFromEvent/TryFromEffect` implementations and `From<T> for Enum`.
- Optional sugar macro `on!(Event::Variant { pattern } => |args| { ... })` to reduce boilerplate.

---

## Performance Considerations
- No HashMaps or runtime registration lookup — only generic chains.
- Tuple type-sets provide O(1) field access; borrow splitting via `get2_mut/get3_mut` uses direct references.
- Handlers are fully monomorphized; unused extractors/code optimized out.
- Avoid trait objects and dynamic dispatch inside hot paths.

---

## Error Handling and Safety
- Duplicate types in model/resource tuples lead to trait ambiguity at compile time (desired).
- Requesting two mutable borrows of the same type is impossible because there is no `get2_mut<T, T>` impl.
- Extractors that cannot be built (e.g., `EventRef<Login>` when event is not a `Login`) result in `None`, making the handler non-applicable in the chain.
- Effect timeouts and panic handling remain in `Shell` (unchanged). `EffectRouter` composes with current timeout logic.

---

## Integration with Existing Core/Shell
- `Core` continues to own the model (now a tuple). `update` returns `Command<E, Fx>` as usual.
- `Shell` continues to execute effects via `EffectHandler`. We provide `EffectRouter` that implements the existing `EffectHandler` trait.
- `Runner` logic remains unchanged; only the `App` implementation changes.

---

## Step-by-Step Implementation Plan

Phase 1 — Foundations (types and traits)
1. Add `src/event_map/mod.rs` and submodules: `typeset.rs`, `extract.rs`, `handler.rs`, `router.rs`, `builder.rs`.
2. Implement type-set traits in `typeset.rs`:
   - `Get<T>`, `GetMut<T>`, `Get2Mut<A,B>`, `Get3Mut<A,B,C>`
   - Macro to implement for tuples up to N=12 (configurable). Include tests.
3. Implement event/effect contexts and extractors in `extract.rs`:
   - `EventCtx`, `EffectCtx`, extractors `Model`, `ModelMut`, `Resource`, `EventRef`, `EffectRef`, `Ctx`
   - `FromEventCtx`, `FromEffectCtx` for each extractor type
   - Traits `TryFromEvent<E>`, `TryFromEffect<Fx>` with basic helper macros to implement for variants
4. Implement argument tuple extraction in `handler.rs`:
   - `ExtractEventArgs` and `ExtractEffectArgs` (arity 0..=6 for MVP)
   - `EventHandler` and `EffectHandlerFn` blanket impls over functions

Phase 2 — Routers and App integration
5. Implement `EventRouter` in `router.rs`:
   - Generic chain type `Chain<Head, Tail>` and terminator `Nil`
   - Methods: `on_event`, `fallback`, `handle`
6. Implement `EffectRouter` in `router.rs`:
   - Generic chain and `EffectHandler` impl bridging to `Shell`
7. Implement `EventMapApp` in `mod.rs` and an `effect_router()` accessor
8. Implement `EventAppBuilder` in `builder.rs`:
   - Tuple-append helpers for models/resources values and types
   - `on_event`, `on_effect`, `build`
9. Re-export in `lib.rs` and prelude; add feature gate `experimental-handlers` if desired initially

Phase 3 — Tests and Examples
10. Unit tests:
    - Type-set getters and multi-borrows
    - Extractors correctness
    - EventRouter first-match semantics
    - EffectRouter handling with a mock resource and event emission via `EffectContext`
11. Examples:
    - `examples/handlers_login.rs` showing multiple models/resources and few event/effect handlers
    - `examples/handlers_effects.rs` showing effect-side emission of events
12. Benchmarks (optional in benches/): micro-bench `Get/GetMut` and router overhead

Phase 4 — Ergonomics and Macros (optional)
13. Add `syzygy_macros` crate (optional) with:
    - `sum!` macro to define enums from structs and derive `TryFromEvent/TryFromEffect`
    - `on!` macro for pattern-friendly handler registration
14. Expand tuple arity if requested (N=16/24)

Phase 5 — Docs and Migration
15. README updates: quickstart for `EventAppBuilder` path
16. Migration guide: moving from a big `update`/`EffectHandler` to declarative handlers
17. API stability review; stabilize behind feature flag or make default once tested

---

## Acceptance Criteria
- Build compiles with `event_map` enabled; existing API unaffected for existing users.
- Event handlers can:
  - Access multiple models mutably in a single handler (distinct types)
  - Access resources immutably
  - Return `Command<E, Fx>`; first matching handler fires
- Effect handlers can:
  - Access resources and the effect value
  - Emit events via `EffectContext<E>`
  - Run under existing `Shell` timeout/runner logic through `EffectHandler` impl
- No runtime maps or dynamic dispatch in hot paths.

---

## Potential Pitfalls and Mitigations
- Duplicate types in tuples: compile-time errors via trait ambiguity; document the constraint.
- Many handlers may lengthen compile times (generic chains); mitigate by grouping handlers per event family or adding a minimal runtime dispatch layer for “cold paths” only if needed later.
- Borrowing >3 mutable models: extend `getN_mut` arities only if proven necessary; recommend designing handlers to touch few models.

---

## CONSENSUS ALTERNATIVE RECOMMENDATIONS

### ✅ **KEEP CURRENT ARCHITECTURE**
The existing TEA pattern is already excellent:
```rust
fn update(&self, event: Event, model: &mut Model) -> Command<Event, Effect>
```

**Current Performance** (measured):
- Event dispatch: ~7-8ns per event
- Command creation: ~5-7ns
- Task spawning: ~4ns (24x faster than previous)
- Compile time: Fast and reasonable

### ✅ **SIMPLE MULTI-MODEL SOLUTION**
Instead of complex tuple type-sets, use explicit structs:

```rust
struct AppState {
    auth: AuthModel,
    profile: ProfileModel,
    settings: SettingsModel,
}

fn update(event: Event, state: &mut AppState) -> Command<Event, Effect> {
    match event {
        Event::Login(login_data) => {
            state.auth.login(login_data);  // Clear, direct field access
            Command::batch([
                Command::effect(Effect::SendEmail { to: "admin@app.com" }),
                Command::effect(Effect::LogActivity { user: state.auth.user_id }),
            ])
        }
        Event::UpdateProfile(profile_data) => {
            state.profile.update(profile_data);
            Command::none()
        }
    }
}
```

**Benefits:**
- Zero runtime cost (direct field access)
- Readable and maintainable
- Easy to test and debug
- Fast compilation
- Clear data flow

### ✅ **OPTIONAL ERGONOMIC ENHANCEMENTS** 
If dependency injection is absolutely needed:
- Keep TEA core for hot paths
- Add optional helpers for non-critical paths only
- Use explicit resource parameters, not magical extraction
- Benchmark everything continuously

---

## ~~Timeline (suggested)~~
~~- Week 1: Phase 1 (types/traits) + baseline tests~~
~~- Week 2: Phase 2 (routers/app/builder) + unit tests~~
~~- Week 3: Examples, docs, optional benches; internal adoption in one example app~~
~~- Week 4: Ergonomic macros (optional) and public docs/migration guide~~

**RECOMMENDATION**: Focus on documentation, examples, and incremental improvements to the existing excellent architecture.

---

## Appendix: Minimal MVP Signatures

```rust
// typeset.rs
pub trait Get<T> { fn get(&self) -> &T; }
pub trait GetMut<T> { fn get_mut(&mut self) -> &mut T; }
pub trait Get2Mut<A, B> { fn get2_mut(&mut self) -> (&mut A, &mut B); }

// extract.rs
pub struct EventCtx<'a, E, M, R> { pub event: &'a E, pub models: &'a mut M, pub resources: &'a R }
pub trait FromEventCtx<'a, E, M, R> { type Output; fn from_ctx(ctx: &'a mut EventCtx<'a, E, M, R>) -> Option<Self::Output>; }

pub struct Model<T>(pub &'static T);      // actual type holds &'a T
pub struct ModelMut<T>(pub &'static mut T);
pub struct Resource<T>(pub &'static T);
pub struct EventRef<T>(pub T);

pub trait TryFromEvent<E> { fn try_from_event(e: &E) -> Option<Self> where Self: Sized; }

// handler.rs
pub trait EventHandler<E, Fx, M, R> { fn call(&self, ctx: &mut EventCtx<E, M, R>) -> Option<Command<E, Fx>>; }

// router.rs
pub struct EventRouter<...> { /* Chain */ }
impl<...> EventRouter<...> { pub fn handle(&self, e: &E, m: &mut M, r: &R) -> Command<E, Fx>; }

// builder.rs
pub struct EventAppBuilder<E, Fx, ...> { /* ... */ }
```

This plan provides a path to a clean, declarative, and zero-cost default API for Syzygy aligned with axum-like ergonomics.

