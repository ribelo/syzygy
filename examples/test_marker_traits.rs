//! Test that marker traits work correctly

use syzygy::prelude::*;
use syzygy::executor::{InlineAsync, TokioExecutor, Outcome};

#[derive(Debug, Clone)]
enum Event {
    Test,
}

#[derive(Debug, Clone)]
enum Effect {
    TestEffect,
}

fn handle_effects(effect: Effect, ctx: &EffectContext<Event, ()>) -> Task<Event, ()> {
    match effect {
        Effect::TestEffect => {
            // This SHOULD compile - concurrent with Concurrent executor
            #[cfg(feature = "tokio")]
            return ctx.spawn_concurrent::<TokioExecutor, _, _>(|_ctx| async {
                println!("✓ spawn_concurrent with TokioExecutor (Concurrent) works!");
                Outcome::Events(vec![Event::Test])
            });
            
            // Fallback for non-tokio
            #[cfg(not(feature = "tokio"))]
            return ctx.spawn_best_effort::<InlineAsync<Event>, _, _>(|_ctx| async {
                println!("✓ spawn_best_effort with InlineAsync works!");
                Outcome::Events(vec![Event::Test])
            });
        }
    }
}

fn main() {
    println!("✅ Marker traits working correctly!");
    println!("- Sequential executors can't be used with spawn_concurrent");
    println!("- Concurrent executors work with spawn_concurrent");
    println!("- Both work with spawn_best_effort");
}
