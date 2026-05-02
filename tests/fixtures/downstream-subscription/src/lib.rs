#![allow(unexpected_cfgs)]

use syzygy::Model;

#[cfg(feature = "shell")]
compile_error!("downstream fixture must not define a local shell feature");

#[derive(Model)]
struct AppModel {
    #[model(wrapper = Counter)]
    counter: i32,
}

fn assert_subscription_part<T: syzygy::extract::SubscriptionPart<AppModel>>() {}

pub fn compile_check() {
    assert_subscription_part::<Counter>();
}
