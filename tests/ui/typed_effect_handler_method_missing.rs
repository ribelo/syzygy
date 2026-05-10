use syzygy::prelude::*;

#[derive(Clone)]
struct DbPool;

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

fn save(_db: DbPool) -> Task<Event, Effect> {
    Task::none()
}

fn main() {
    let _runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .event_handler(handle_event)
        .effect_handler(|effect, ctx| match effect {
            Effect::Save => handle!(save, ctx),
        })
        .build()
        .unwrap();
}
