#![allow(clippy::enum_variant_names)]

use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
    log: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    StartSequence,
    LogA,
    LogB,
    ChildEvent(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Effect {
    TriggerA,
    TriggerB,
    ChildEffect(String),
}

// Dummy child types for mapping example
#[derive(Debug, Clone, PartialEq, Eq)]
enum ChildEvent {
    DidThing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ChildEffect {
    Alert,
}

/// ## Why use `Command::and`?
///
/// `Command::and` allows merging two distinct commands into one. This is
/// useful when you receive a command from a helper function and want to
/// append more steps.
fn start_sequence(_: ()) -> Command<Event, Effect> {
    let cmd1 = Command::event(Event::LogA).and_effect(Effect::TriggerA);
    let cmd2 = Command::event(Event::LogB).and_effect(Effect::TriggerB);

    // Merge commands together
    let merged = cmd1.and(cmd2);

    // ## Why `map_event` and `map_effect`?
    //
    // Command mapping is how we reuse components or helpers. If we have a
    // generic command but need to wrap its events/effects in our enums,
    // we map them.
    let mapped = create_generic_command()
        .map_event(|_| Event::ChildEvent("Mapped!".into()))
        .map_effect(|_| Effect::ChildEffect("Alert!".into()));

    merged.and(mapped)
}

fn create_generic_command() -> Command<ChildEvent, ChildEffect> {
    Command::event(ChildEvent::DidThing).and_effect(ChildEffect::Alert)
}

fn log_a(_: (), log: &mut Log) -> Command<Event, Effect> {
    log.push("A".into());
    // ## Why `Command::none()`?
    // Represents a command that does nothing.
    Command::none()
}

fn log_b(_: (), log: &mut Log) -> Command<Event, Effect> {
    log.push("B".into());
    Command::none()
}

fn log_child_event(s: String, log: &mut Log) -> Command<Event, Effect> {
    log.push(format!("Child: {s}"));
    Command::none()
}

fn handle_event(event: Event, ctx: &EventContext<AppModel>) -> Command<Event, Effect> {
    match event {
        Event::StartSequence => handle!(start_sequence, ctx),
        Event::LogA => handle!(log_a, ctx),
        Event::LogB => handle!(log_b, ctx),
        Event::ChildEvent(s) => handle!(log_child_event, s, ctx),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(AppModel::default())
        .event_handler(handle_event)
        .build();

    runner.core().try_send_event(Event::StartSequence)?;

    // Process StartSequence
    runner.step()?;
    // StartSequence pushes LogA, LogB and ChildEvent events into the queue.
    // They will be processed in the next steps.
    runner.step()?;

    println!("Log: {:?}", runner.model().log);
    Ok(())
}
