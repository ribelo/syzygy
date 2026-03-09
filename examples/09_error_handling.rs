use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
    #[model(wrapper = Status)]
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    DoRiskyWork,
    WorkSuccess,
    /// ## Why pass Result in events?
    ///
    /// Effect handlers shouldn't panic on expected operational errors (like I/O).
    /// Instead, they capture the `Result` and map it into an Event so the Core
    /// can decide how to update the state (e.g. showing an error banner).
    WorkFailed(String), // Recoverable errors become data in events
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Effect {
    PerformRiskyWork,
}

fn do_risky_work(_: (), status: &mut Status) -> Command<Event, Effect> {
    **status = "Working...".into();
    Command::effect(Effect::PerformRiskyWork)
}

fn work_success(_: (), status: &mut Status) -> Command<Event, Effect> {
    **status = "Success!".into();
    Command::none()
}

/// ## Why handle errors with events?
/// In TEA, if an asynchronous operation fails, it emits an error event.
/// The pure core logic then catches this event and gracefully degrades the UI.
fn work_failed(err: String, status: &mut Status) -> Command<Event, Effect> {
    **status = format!("Failed: {err}");
    Command::none()
}

fn handle_event(event: Event, ctx: &EventContext<AppModel>) -> Command<Event, Effect> {
    match event {
        Event::DoRiskyWork => handle!(do_risky_work, ctx),
        Event::WorkSuccess => handle!(work_success, ctx),
        Event::WorkFailed(err) => handle!(work_failed, ctx, err),
    }
}

/// ## Why use `Result` in Effect Handlers?
///
/// Real-world operations (like HTTP, File I/O) return `Result`. Effect handlers
/// map the `Result` into the corresponding Success or Failure `Command`.
fn perform_risky_work() -> Task<Event, Effect> {
    Task::once(async {
        // Simulate an operation that might fail
        let result: Result<(), &str> = Err("Network unavailable");

        match result {
            Ok(()) => Command::event(Event::WorkSuccess),
            Err(e) => Command::event(Event::WorkFailed(e.to_string())),
        }
    })
}

fn handle_effect(effect: Effect, ctx: &EffectContext<'_>) -> Task<Event, Effect> {
    match effect {
        Effect::PerformRiskyWork => handle!(perform_risky_work, ctx),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(AppModel::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build()?;

    runner.core().emit(Event::DoRiskyWork);

    // We step to trigger the effect.
    runner.step()?;

    // The owned runtime drives the async effect on the next step.
    runner.step()?;

    println!("Current Status: {}", runner.model().status);
    Ok(())
}
