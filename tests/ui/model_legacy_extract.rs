use syzygy::prelude::Model;

#[derive(Model)]
struct LegacyExtractModel {
    #[extract]
    value: i32,
}

fn main() {}
