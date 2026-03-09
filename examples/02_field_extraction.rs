use syzygy::prelude::*;

/// A custom struct that we want to extract directly, without a wrapper.
#[derive(Debug, Default, Clone)]
struct UserProfile {
    name: String,
}

#[derive(Debug, Default, Model)]
struct AppState {
    /// ## Why use an explicit wrapper here?
    ///
    /// For primitive types like `i32` or `String`, multiple fields might have the same type.
    /// `#[model(wrapper = Score)]` creates a unique extractor type so the runtime
    /// borrow checker can distinguish this field from other `i32` values.
    #[model(wrapper = Score)]
    score: i32,

    /// ## Why use `#[model(part)]`?
    ///
    /// By adding `#[model(part)]`, the `Model` macro implements the internal extraction traits
    /// directly on the `UserProfile` type rather than generating a wrapper.
    /// This allows us to access the complex struct directly in our handlers.
    ///
    /// Constraint: You can only have one `#[model(part)]` field per type in a given model.
    #[model(part)]
    profile: UserProfile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    AddPoints(i32),
    UpdateName(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {}

/// ## Why standard wrappers?
///
/// Notice `&mut Score`. We use the generated wrapper, keeping it distinct from
/// any other `i32` fields in the model.
fn add_points(points: i32, score: &mut Score) -> Command<AppEvent, AppEffect> {
    **score += points;
    Command::none()
}

/// ## Why extracted fields?
///
/// Notice `&mut UserProfile`. We take the raw struct directly.
/// No double dereferencing is needed. We interact with it just like normal Rust.
fn update_name(name: String, profile: &mut UserProfile) -> Command<AppEvent, AppEffect> {
    profile.name = name;
    Command::none()
}

fn handle_event(event: AppEvent, ctx: &EventContext<AppState>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::AddPoints(amount) => handle!(add_points, ctx, amount),
        AppEvent::UpdateName(name) => handle!(update_name, ctx, name),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppState::default())
        .event_handler(handle_event)
        .build()?;

    app.core().try_send(AppEvent::UpdateName("Alice".into()))?;
    app.core().try_send(AppEvent::AddPoints(100))?;

    app.step()?;
    app.step()?;

    println!(
        "User: {}, Score: {}",
        app.model().profile.name,
        app.model().score
    );
    Ok(())
}
