use syzygy::prelude::*;

#[derive(Debug, Clone)]
enum Event {
    Increment(u32),
    Rename(String),
    TriggerSave,
    DoubleBorrow,
}

#[derive(Debug, Clone)]
enum Effect {
    Save,
}

#[derive(Model)]
struct AppModel {
    counter: i32,
    display_name: String,
}

#[derive(Clone, Resources)]
struct AppResources {
    db_url: String,
    save_completed: bool,
}

fn increment(amount: u32, mut counter: Counter) -> Command<Event, Effect> {
    let amount = i32::try_from(amount).expect("u32 amount must fit in i32");
    *counter += amount;
    Command::none()
}

fn rename(new_name: String, mut display_name: DisplayName) -> Command<Event, Effect> {
    *display_name = new_name;
    Command::none()
}

fn trigger_save(_: ()) -> Command<Event, Effect> {
    Command::effect(Effect::Save)
}

fn double_borrow(_: (), _first: Counter, _second: Counter) -> Command<Event, Effect> {
    unreachable!()
}

fn persist(_: (), db_url: DbUrl, save_completed: SaveCompleted) -> Task<Event, Effect> {
    assert_eq!(db_url.0, "pg://test");
    assert!(!save_completed.0);
    Task::none()
}

#[test]
fn derive_model_and_resources_work_in_runtime() {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(AppModel {
            counter: 0,
            display_name: "init".into(),
        })
        .with_resources(AppResources {
            db_url: "pg://test".into(),
            save_completed: false,
        })
        .event_handler(|event: Event, ctx: &EventContext<AppModel>| match event {
            Event::Increment(amount) => increment.handle(amount, ctx),
            Event::Rename(name) => rename.handle(name, ctx),
            Event::TriggerSave => trigger_save.handle((), ctx),
            Event::DoubleBorrow => double_borrow.handle((), ctx),
        })
        .effect_handler(
            |effect: Effect, ctx: &EffectContext<AppResources>| match effect {
                Effect::Save => persist.handle((), ctx),
            },
        )
        .async_executor(InlineAsync::new())
        .build();

    runner.core().try_send_event(Event::Increment(3)).unwrap();
    runner.step().unwrap();
    assert_eq!(runner.model().counter, 3);

    runner
        .core()
        .try_send_event(Event::Rename("next".into()))
        .unwrap();
    runner.step().unwrap();
    assert_eq!(runner.model().display_name, "next");

    runner.core().try_send_event(Event::TriggerSave).unwrap();
    runner.step().unwrap();
}

#[test]
fn derive_model_tracks_runtime_borrows() {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(AppModel {
            counter: 0,
            display_name: "init".into(),
        })
        .with_resources(AppResources {
            db_url: "pg://test".into(),
            save_completed: false,
        })
        .event_handler(|event: Event, ctx: &EventContext<AppModel>| match event {
            Event::Increment(amount) => increment.handle(amount, ctx),
            Event::Rename(name) => rename.handle(name, ctx),
            Event::TriggerSave => trigger_save.handle((), ctx),
            Event::DoubleBorrow => double_borrow.handle((), ctx),
        })
        .effect_handler(
            |effect: Effect, ctx: &EffectContext<AppResources>| match effect {
                Effect::Save => persist.handle((), ctx),
            },
        )
        .async_executor(InlineAsync::new())
        .build();

    assert_panic_contains("already borrowed mutably", || {
        runner.core().try_send_event(Event::DoubleBorrow).unwrap();
        let _ = runner.step();
    });
}
