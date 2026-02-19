use std::ops::{Deref, DerefMut};

use syzygy::prelude::*;

#[derive(Debug, Default)]
struct Model {
    counter: i32,
    saving: bool,
    last_saved: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    Increment(i32),
    Save,
    SaveDone(i32),
    SaveFailed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Effect {
    SaveToServer(i32),
}

#[derive(Debug, Clone)]
struct Resources {
    server_url: String,
}

struct Counter(*mut i32);

impl Deref for Counter {
    type Target = i32;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Pointer is created from `Model::counter` for the active model.
        unsafe { &*self.0 }
    }
}

impl DerefMut for Counter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Pointer is created from `Model::counter` and borrow tracking enforces exclusivity.
        unsafe { &mut *self.0 }
    }
}

impl FromEventContext<Model> for Counter {
    fn from_context(ctx: &EventContext<Model>) -> Self {
        ctx.track_borrow(0, "counter");
        // SAFETY: Field index 0 is reserved for `Model::counter`.
        Counter(unsafe { &mut (*ctx.model_ptr()).counter })
    }
}

struct Saving(*mut bool);

impl Deref for Saving {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Pointer is created from `Model::saving` for the active model.
        unsafe { &*self.0 }
    }
}

impl DerefMut for Saving {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Pointer is created from `Model::saving` and borrow tracking enforces exclusivity.
        unsafe { &mut *self.0 }
    }
}

impl FromEventContext<Model> for Saving {
    fn from_context(ctx: &EventContext<Model>) -> Self {
        ctx.track_borrow(1, "saving");
        // SAFETY: Field index 1 is reserved for `Model::saving`.
        Saving(unsafe { &mut (*ctx.model_ptr()).saving })
    }
}

struct LastSaved(*mut Option<i32>);

impl Deref for LastSaved {
    type Target = Option<i32>;

    fn deref(&self) -> &Self::Target {
        // SAFETY: Pointer is created from `Model::last_saved` for the active model.
        unsafe { &*self.0 }
    }
}

impl DerefMut for LastSaved {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: Pointer is created from `Model::last_saved` and borrow tracking enforces exclusivity.
        unsafe { &mut *self.0 }
    }
}

impl FromEventContext<Model> for LastSaved {
    fn from_context(ctx: &EventContext<Model>) -> Self {
        ctx.track_borrow(2, "last_saved");
        // SAFETY: Field index 2 is reserved for `Model::last_saved`.
        LastSaved(unsafe { &mut (*ctx.model_ptr()).last_saved })
    }
}

#[derive(Clone)]
struct ServerUrl(String);

impl FromEffectContext<Resources> for ServerUrl {
    fn from_context(ctx: &EffectContext<Resources>) -> Self {
        Self(ctx.resources().server_url.clone())
    }
}

fn increment(amount: i32, mut counter: Counter) -> Command<Event, Effect> {
    *counter += amount;
    Command::none()
}

fn save(_: (), mut saving: Saving, counter: Counter) -> Command<Event, Effect> {
    *saving = true;
    Command::effect(Effect::SaveToServer(*counter))
}

fn save_done(value: i32, mut saving: Saving, mut last_saved: LastSaved) -> Command<Event, Effect> {
    *saving = false;
    *last_saved = Some(value);
    Command::none()
}

fn save_failed(msg: String, mut saving: Saving) -> Command<Event, Effect> {
    *saving = false;
    eprintln!("save failed: {msg}");
    Command::none()
}

fn save_to_server(value: i32, url: ServerUrl) -> Task<Event, Effect> {
    if url.0.is_empty() {
        Task::event(Event::SaveFailed("server url is empty".to_string()))
    } else {
        Task::event(Event::SaveDone(value))
    }
}

fn dispatch_event(event: Event, ctx: &EventContext<Model>) -> Command<Event, Effect> {
    match event {
        Event::Increment(amount) => increment.handle(amount, ctx),
        Event::Save => save.handle((), ctx),
        Event::SaveDone(value) => save_done.handle(value, ctx),
        Event::SaveFailed(msg) => save_failed.handle(msg, ctx),
    }
}

fn dispatch_effect(effect: Effect, ctx: &EffectContext<Resources>) -> Task<Event, Effect> {
    match effect {
        Effect::SaveToServer(value) => save_to_server.handle(value, ctx),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(Model::default())
        .with_resources(Resources {
            server_url: "https://api.example.test/counter".to_string(),
        })
        .event_handler(dispatch_event)
        .effect_handler(dispatch_effect)
        .build();

    runner.core().try_send_event(Event::Increment(3))?;
    runner.step()?;

    runner.core().try_send_event(Event::Save)?;
    runner.step()?;
    runner.step()?;

    let model = runner.model();
    println!(
        "counter={}, saving={}, last_saved={:?}",
        model.counter, model.saving, model.last_saved
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_task_events(store: &mut TestStore<Event, Effect, Model>, task: Task<Event, Effect>) {
        match task {
            Task::Event(event) => store.send(event),
            Task::Events(events) => {
                for event in events {
                    store.send(event);
                }
            }
            Task::Async { .. }
            | Task::Stream { .. }
            | Task::Blocking { .. }
            | Task::BlockingWithResource { .. } => panic!("expected immediate event task"),
        }
    }

    #[test]
    fn increment_updates_counter_without_effects() {
        let mut store = TestStore::new(Model::default(), dispatch_event);

        store.send(Event::Increment(4));

        assert_eq!(store.state().counter, 4);
        store.assert_no_effects();
    }

    #[test]
    fn save_sets_saving_and_emits_effect() {
        let mut store = TestStore::new(
            Model {
                counter: 7,
                saving: false,
                last_saved: None,
            },
            dispatch_event,
        );

        store.send(Event::Save);

        assert!(store.state().saving);
        store.assert_effects([Effect::SaveToServer(7)]);
    }

    #[test]
    fn full_cycle_updates_last_saved() {
        let mut store = TestStore::new(Model::default(), dispatch_event);
        let effect_ctx = EffectContext::new(Resources {
            server_url: "https://api.example.test/counter".to_string(),
        });

        store.send(Event::Increment(9));
        store.send(Event::Save);

        let effects = store.take_effects();
        assert_eq!(effects, vec![Effect::SaveToServer(9)]);

        for effect in effects {
            let task = dispatch_effect(effect, &effect_ctx);
            run_task_events(&mut store, task);
        }

        assert_eq!(store.state().last_saved, Some(9));
        assert!(!store.state().saving);
        store.assert_no_effects();
    }

    #[test]
    fn save_failed_event_clears_saving() {
        let mut store = TestStore::new(
            Model {
                counter: 1,
                saving: true,
                last_saved: None,
            },
            dispatch_event,
        );

        store.send(Event::SaveFailed("network error".to_string()));

        assert!(!store.state().saving);
        store.assert_no_effects();
    }
}
