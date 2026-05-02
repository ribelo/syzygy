use syzygy::Model;

#[derive(Model)]
struct AppModel {
    #[model(wrapper = Counter)]
    counter: i32,
}

fn assert_subscription_part<T: syzygy::extract::SubscriptionPart<AppModel>>() {}

fn main() {
    assert_subscription_part::<Counter>();
}
