//! Simple test for the derive macro

use syzygy::prelude::*;

#[derive(ModelExtractors)]
struct TestModel {
    #[extract]
    counter: i32,
    
    #[extract(as = TestName)]
    name: String,
}

fn main() {
    println!("Testing derive macro");
}