use syzygy::prelude::Model;

#[derive(Model)]
struct DuplicatePartModel {
    #[model(part)]
    first: i32,
    #[model(part)]
    second: i32,
}

fn main() {}
