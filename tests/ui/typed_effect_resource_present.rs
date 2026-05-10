use syzygy::prelude::*;

#[derive(Clone)]
struct DbPool(&'static str);

#[derive(Clone)]
enum Event {
    Start,
}

#[derive(Clone)]
enum Effect {
    Save,
}

fn handle_event(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::Start => Command::effect(Effect::Save),
    }
}

fn save(_: (), db: DbPool) -> Task<Event, Effect> {
    assert_eq!(db.0, "primary");
    Task::none()
}

fn main() {
    let _runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .with_resource(DbPool("primary"))
        .event_handler(handle_event)
        .typed_effect_handler(|effect, ctx| match effect {
            Effect::Save => handle!(save, ctx),
        })
        .build()
        .unwrap();
}
