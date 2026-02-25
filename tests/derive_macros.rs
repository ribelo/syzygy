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

#[derive(Clone)]
struct DbUrl(String);

#[derive(Clone)]
struct SaveCompleted(bool);

impl DbUrl {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl SaveCompleted {
    fn is_done(&self) -> bool {
        self.0
    }
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
    Command::none()
}

fn persist(_: (), db_url: DbUrl, save_completed: SaveCompleted) -> Task<Event, Effect> {
    assert_eq!(db_url.as_str(), "pg://test");
    assert!(!save_completed.is_done());
    Task::none()
}

#[test]
fn derive_model_works_in_runtime() {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(AppModel {
            counter: 0,
            display_name: "init".into(),
        })
        .with_resource(DbUrl("pg://test".into()))
        .with_resource(SaveCompleted(false))
        .event_handler(|event: Event, ctx: &EventContext<AppModel>| match event {
            Event::Increment(amount) => increment.handle(amount, ctx),
            Event::Rename(name) => rename.handle(name, ctx),
            Event::TriggerSave => trigger_save.handle((), ctx),
            Event::DoubleBorrow => double_borrow.handle((), ctx),
        })
        .effect_handler(|effect: Effect, ctx: &EffectContext| match effect {
            Effect::Save => persist.handle((), ctx),
        })
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
fn derive_model_allows_double_wrapper_projection() {
    let mut runner = Syzygy::builder::<Event, Effect>()
        .model(AppModel {
            counter: 0,
            display_name: "init".into(),
        })
        .with_resource(DbUrl("pg://test".into()))
        .with_resource(SaveCompleted(false))
        .event_handler(|event: Event, ctx: &EventContext<AppModel>| match event {
            Event::Increment(amount) => increment.handle(amount, ctx),
            Event::Rename(name) => rename.handle(name, ctx),
            Event::TriggerSave => trigger_save.handle((), ctx),
            Event::DoubleBorrow => double_borrow.handle((), ctx),
        })
        .effect_handler(|effect: Effect, ctx: &EffectContext| match effect {
            Effect::Save => persist.handle((), ctx),
        })
        .build();

    runner.core().try_send_event(Event::DoubleBorrow).unwrap();
    runner.step().unwrap();
}

#[derive(Debug, Clone)]
enum ExtractEvent {
    Increment(i32),
    Rename(String),
    UpdateChildren,
    DoubleBorrow,
}

#[derive(Debug, Clone)]
enum ExtractEffect {}

#[derive(Model)]
struct CounterState {
    value: i32,
}

#[derive(Model)]
struct ToggleState {
    enabled: bool,
}

#[derive(Model)]
struct ExtractAppModel {
    #[extract]
    counter: CounterState,
    #[extract]
    toggle: ToggleState,
    title: String,
}

fn increment_child(
    amount: i32,
    counter: &mut CounterState,
) -> Command<ExtractEvent, ExtractEffect> {
    counter.value += amount;
    Command::none()
}

fn rename_extract(new_title: String, mut title: Title) -> Command<ExtractEvent, ExtractEffect> {
    *title = new_title;
    Command::none()
}

fn update_children(
    counter: &mut CounterState,
    toggle: &mut ToggleState,
) -> Command<ExtractEvent, ExtractEffect> {
    counter.value += 10;
    toggle.enabled = !toggle.enabled;
    Command::none()
}

fn double_borrow_child(
    _first: &mut CounterState,
    _second: &mut CounterState,
) -> Command<ExtractEvent, ExtractEffect> {
    unreachable!()
}

#[test]
fn derive_model_extract_attribute_projects_child_models() {
    let mut runner = Syzygy::builder::<ExtractEvent, ExtractEffect>()
        .model(ExtractAppModel {
            counter: CounterState { value: 1 },
            toggle: ToggleState { enabled: false },
            title: "init".to_string(),
        })
        .event_handler(
            |event: ExtractEvent, ctx: &EventContext<ExtractAppModel>| match event {
                ExtractEvent::Increment(amount) => increment_child.handle(amount, ctx),
                ExtractEvent::Rename(new_title) => rename_extract.handle(new_title, ctx),
                ExtractEvent::UpdateChildren => update_children.handle((), ctx),
                ExtractEvent::DoubleBorrow => double_borrow_child.handle((), ctx),
            },
        )
        .effect_handler(|_effect: ExtractEffect, _ctx: &EffectContext| {
            Task::<ExtractEvent, ExtractEffect>::none()
        })
        .build();

    runner
        .core()
        .try_send_event(ExtractEvent::Increment(4))
        .unwrap();
    runner.step().unwrap();
    assert_eq!(runner.model().counter.value, 5);

    runner
        .core()
        .try_send_event(ExtractEvent::UpdateChildren)
        .unwrap();
    runner.step().unwrap();
    assert_eq!(runner.model().counter.value, 15);
    assert!(runner.model().toggle.enabled);

    runner
        .core()
        .try_send_event(ExtractEvent::Rename("updated".to_string()))
        .unwrap();
    runner.step().unwrap();
    assert_eq!(runner.model().title, "updated");
}

#[test]
fn derive_model_extract_attribute_detects_overlap() {
    let mut runner = Syzygy::builder::<ExtractEvent, ExtractEffect>()
        .model(ExtractAppModel {
            counter: CounterState { value: 0 },
            toggle: ToggleState { enabled: false },
            title: "init".to_string(),
        })
        .event_handler(
            |event: ExtractEvent, ctx: &EventContext<ExtractAppModel>| match event {
                ExtractEvent::Increment(amount) => increment_child.handle(amount, ctx),
                ExtractEvent::Rename(new_title) => rename_extract.handle(new_title, ctx),
                ExtractEvent::UpdateChildren => update_children.handle((), ctx),
                ExtractEvent::DoubleBorrow => double_borrow_child.handle((), ctx),
            },
        )
        .effect_handler(|_effect: ExtractEffect, _ctx: &EffectContext| {
            Task::<ExtractEvent, ExtractEffect>::none()
        })
        .build();

    assert_panic_contains("overlaps already-borrowed mutable region", || {
        runner
            .core()
            .try_send_event(ExtractEvent::DoubleBorrow)
            .unwrap();
        let _ = runner.step();
    });
}
