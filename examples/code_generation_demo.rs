//! Code generation demonstration
//!
//! This example shows the exact code that would be generated for zero-overhead
//! event dispatching using the new on_event<T>() API.

use syzygy::prelude::*;
use syzygy::codegen::generate_code_string;

// ============================================================================
// Test Events
// ============================================================================

#[derive(Debug, Clone)]
struct CreateUser { name: String }

#[derive(Debug, Clone)]
struct UpdateCounter { increment: i32 }

#[derive(Debug, Clone)]
enum TestEvent {
    CreateUser(CreateUser),
    UpdateCounter(UpdateCounter),
    UserCreated { name: String },
    CounterUpdated { value: i32 },
}

#[derive(Debug, Clone)]
enum TestCommand {
    SaveUser { name: String },
    LogMessage { message: String },
}

// From trait implementations
impl From<CreateUser> for TestEvent {
    fn from(event: CreateUser) -> Self {
        TestEvent::CreateUser(event)
    }
}

impl From<UpdateCounter> for TestEvent {
    fn from(event: UpdateCounter) -> Self {
        TestEvent::UpdateCounter(event)
    }
}

impl TryFrom<TestEvent> for CreateUser {
    type Error = ();
    fn try_from(event: TestEvent) -> Result<Self, Self::Error> {
        match event {
            TestEvent::CreateUser(inner) => Ok(inner),
            _ => Err(()),
        }
    }
}

impl TryFrom<TestEvent> for UpdateCounter {
    type Error = ();
    fn try_from(event: TestEvent) -> Result<Self, Self::Error> {
        match event {
            TestEvent::UpdateCounter(inner) => Ok(inner),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Default)]
struct TestModel {
    users: Vec<String>,
    counter: i32,
}

fn main() {
    println!("🔧 Zero-Overhead Code Generation Demo");
    println!("=====================================\n");

    // Create builder with event handlers
    let builder = Syzygy::builder()
        .model(TestModel::default())
        .on_event::<CreateUser>(|event| {
            Dispatch::new(
                vec![TestEvent::UserCreated { name: event.name.clone() }],
                vec![TestCommand::SaveUser { name: event.name }],
            )
        })
        .on_event::<UpdateCounter>(|event| {
            Dispatch::new(
                vec![TestEvent::CounterUpdated { value: event.increment }],
                vec![TestCommand::LogMessage {
                    message: format!("Incremented by {}", event.increment)
                }],
            )
        });

    // Show collected metadata
    println!("📋 Collected Handler Metadata:");
    for handler in &builder.event_handler_storage.handlers {
        println!("  • {} → {}", handler.event_type_name, handler.handler_function_name);
    }

    // Generate and format the code
    let generated_code = generate_code_string(&builder.event_handler_storage.handlers);

    // Pretty print the generated code with proper formatting
    println!("\n📝 Generated Zero-Overhead Code:");
    println!("{}", "=".repeat(80));

    // Parse and reformat the generated code for better readability
    let formatted_code = format_generated_code(&generated_code);
    println!("{}", formatted_code);

    println!("{}", "=".repeat(80));

    println!("\n🚀 What This Achieves:");
    println!("  ✅ Zero runtime overhead - pure match statement");
    println!("  ✅ Compile-time handler registration");
    println!("  ✅ Type-safe event dispatch");
    println!("  ✅ No HashMap lookups");
    println!("  ✅ Direct function calls");

    println!("\n⚡ Performance Target:");
    println!("  🎯 Event dispatch: <200ps");
    println!("  🎯 Model access: <50ps");
    println!("  🎯 Zero allocations");

    println!("\n🔮 Next Steps:");
    println!("  1. Integrate with build.rs for automatic generation");
    println!("  2. Add proc macro for seamless usage");
    println!("  3. Benchmark against current implementation");
    println!("  4. Add magic parameter extraction to generated handlers");
}

// Helper function to format the generated code for better readability
fn format_generated_code(code: &str) -> String {
    // This is a simple formatter - in a real implementation you'd use syn + prettyplease
    code.replace(" # ", "\n# ")
        .replace(" { ", " {\n    ")
        .replace(" } ", "\n}\n")
        .replace(" ; ", ";\n    ")
        .replace("pub fn ", "\npub fn ")
        .replace("pub mod ", "\npub mod ")
        .replace("match event {", "match event {\n        ")
        .replace("), ", "),\n        ")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}
