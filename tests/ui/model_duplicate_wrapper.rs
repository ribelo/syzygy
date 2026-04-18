use syzygy::prelude::Model;

#[derive(Model)]
struct DuplicateWrapperModel {
    #[model(wrapper = Count)]
    left: i32,
    #[model(wrapper = Count)]
    right: u32,
}

fn main() {}
