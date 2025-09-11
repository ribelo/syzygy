//! Example demonstrating the IntoCommand extension trait for ergonomic command creation.

use syzygy::prelude::*;

// Define your events and effects
#[derive(Debug, Clone)]
enum Event {
    UserClicked,
    DataReceived(String),
    Error(String),
}

#[derive(Debug, Clone)]
enum Effect {
    FetchData,
    LogMessage(String),
}

// Implement the IntoCommand trait for your types
impl IntoCommand<Event, Effect> for Event {
    fn cmd(self) -> Command<Event, Effect> {
        Command::event(self)
    }
}

impl IntoCommand<Event, Effect> for Effect {
    fn cmd(self) -> Command<Event, Effect> {
        Command::effect(self)
    }
}

// Example model
#[derive(Debug, Default)]
struct Model {
    data: String,
    error: Option<String>,
}

// Example update function using the ergonomic .cmd() method
fn update(event: Event, ctx: &mut EventContext<Event, Effect, Model>) -> Command<Event, Effect> {
    let model = ctx.model_mut();

    match event {
        Event::UserClicked => {
            // Use .cmd() for ergonomic command creation
            Effect::FetchData.cmd()
        }
        Event::DataReceived(data) => {
            model.data = data;
            // Chain commands using .cmd()
            Effect::LogMessage("Data received".to_string()).cmd()
        }
        Event::Error(msg) => {
            model.error = Some(msg);
            // No side effects needed, return empty command
            Command::none()
        }
    }
}

fn main() {
    println!("IntoCommand extension trait example compiled successfully!");
    println!("You can now use .cmd() method on your events and effects for ergonomic command creation.");
}