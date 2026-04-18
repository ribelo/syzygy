use syzygy::prelude::Model;

#[repr(packed)]
#[derive(Model)]
struct PackedModel {
    #[model(part)]
    value: i32,
}

fn main() {}
