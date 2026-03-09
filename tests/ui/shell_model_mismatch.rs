use syzygy::prelude::*;

#[derive(Clone)]
enum Event {}

#[derive(Model)]
struct ModelA {
    #[model(wrapper = FlagA)]
    flag: bool,
}

#[derive(Model)]
struct ModelB {
    #[model(wrapper = FlagB)]
    flag: bool,
}

fn handle_event_a(_event: Event, _ctx: &EventContext<ModelA>) -> Command<Event, ()> {
    Command::none()
}

fn handle_event_b(_event: Event, _ctx: &EventContext<ModelB>) -> Command<Event, ()> {
    Command::none()
}

fn handle_subscriptions_a(_ctx: &SubscriptionContext<ModelA>) -> Subscription<Event, ()> {
    Subscription::none()
}

fn handle_subscriptions_b(_ctx: &SubscriptionContext<ModelB>) -> Subscription<Event, ()> {
    Subscription::none()
}

fn main() {
    let app_a = Syzygy::builder::<Event, ()>()
        .model(ModelA { flag: false })
        .event_handler(handle_event_a)
        .subscription_handler(handle_subscriptions_a)
        .build()
        .unwrap();

    let app_b = Syzygy::builder::<Event, ()>()
        .model(ModelB { flag: false })
        .event_handler(handle_event_b)
        .subscription_handler(handle_subscriptions_b)
        .build()
        .unwrap();

    let (_core_a, shell_a) = app_a.split();
    let (core_b, _shell_b) = app_b.split();
    let _bad = Syzygy::from((core_b, shell_a));
}
