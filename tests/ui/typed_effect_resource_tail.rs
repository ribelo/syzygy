use syzygy::prelude::*;

#[derive(Clone)]
struct DbPool(&'static str);

#[derive(Clone)]
struct HttpClient(&'static str);

#[derive(Clone)]
enum Event {
    Start,
}

#[derive(Clone)]
enum Effect {
    Sync,
}

fn handle_event(event: Event, _ctx: &EventContext<()>) -> Command<Event, Effect> {
    match event {
        Event::Start => Command::effect(Effect::Sync),
    }
}

fn sync(_: (), db: DbPool, http: HttpClient) -> Task<Event, Effect> {
    assert_eq!(db.0, "primary");
    assert_eq!(http.0, "api");
    Task::none()
}

fn main() {
    let _runner = Syzygy::builder::<Event, Effect>()
        .model(())
        .with_resource(HttpClient("api"))
        .with_resource(DbPool("primary"))
        .event_handler(handle_event)
        .typed_effect_handler(|effect, ctx| match effect {
            Effect::Sync => handle!(sync, ctx),
        })
        .build()
        .unwrap();
}
