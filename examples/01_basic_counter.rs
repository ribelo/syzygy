use syzygy::prelude::*;

/// ## Why use `#[derive(Model)]`?
///
/// The `Model` macro is the core of Syzygy's state management.
/// Field extraction is explicit: `#[model(wrapper = Counter)]` opts this field
/// into a named wrapper type that the runtime can track independently.
#[derive(Debug, Default, Model)]
struct AppModel {
    #[model(wrapper = Counter)]
    counter: i32,
}

/// ## Why an Enum for Events?
///
/// Events represent "things that happened" in our application.
/// Using an enum ensures all possible events are explicitly defined and
/// can be exhaustively matched in our `handle_event` function.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    Increment,
    Decrement,
}

/// ## Why define an Effect type even if empty?
///
/// Effects represent side-effects (like HTTP calls or file I/O).
/// Even in a pure application with no side effects, Syzygy requires an
/// Effect type to satisfy its internal runtime type signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {}

/// ## Why use wrapper types?
///
/// `#[model(wrapper = Counter)]` generated a transparent wrapper type for this field.
/// The wrapper implements `Deref` and `DerefMut`, allowing us to use `**counter`
/// to access the underlying `i32` while keeping extraction explicit in the model.
///
/// This approach enables the runtime borrow checker to prevent aliasing
/// violations at the field level.
fn increment(counter: &mut Counter) -> Command<AppEvent, AppEffect> {
    // Double deref: &mut Counter -> &mut i32 -> i32
    // The first * goes through DerefMut, the second derefs the &mut i32
    **counter += 1;
    Command::none()
}

/// ## Why a pure handler?
///
/// Notice this function returns `Command::none()`. It updates the state (`counter`)
/// but does not produce any further events or effects. Handlers should generally be
/// pure (side-effect free), delegating external actions to `Task`s via `Effect`s.
fn decrement(counter: &mut Counter) -> Command<AppEvent, AppEffect> {
    **counter -= 1;
    Command::none()
}

/// ## Why a central `handle_event` function?
///
/// The `handle_event` function acts as a router. Syzygy sends all `AppEvent`
/// instances here, and we delegate to specific handlers. We use
/// `handle!(handler, ctx)` to invoke handlers, injecting the parsed context
/// explicitly.
fn handle_event(event: AppEvent, ctx: &EventContext<AppModel>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Increment => handle!(increment, ctx),
        AppEvent::Decrement => handle!(decrement, ctx),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ## Building the Syzygy runtime
    // We bind our `AppModel`, event handler, and effect handler together.
    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(handle_event)
        .build()?;

    // ## Running the application
    // We can inject events manually using the core channel.
    app.core().try_send(AppEvent::Increment)?;
    // `step()` processes one pass of the internal event queue.
    app.step()?;

    app.core().try_send(AppEvent::Increment)?;
    app.step()?;

    app.core().try_send(AppEvent::Decrement)?;
    app.step()?;

    println!("Final Counter: {}", app.model().counter);
    Ok(())
}
