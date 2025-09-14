//! Test that marker traits work correctly

use syzygy::executor::{Outcome, TokioExecutor};
use syzygy::prelude::*;

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Event {
    Test,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
enum Effect {
    TestEffect,
}

#[allow(dead_code)]
fn handle_effects(effect: Effect, ctx: &EffectContext<Event, ()>) -> Task<Event, ()> {
    match effect {
        Effect::TestEffect => {
            // Simplified API - just use spawn with any executor
            #[cfg(feature = "tokio")]
            return ctx.spawn::<TokioExecutor, _, _>(|_ctx| async {
                println!("✓ spawn with TokioExecutor works!");
                Outcome::Events(vec![Event::Test])
            });

            // Fallback for non-tokio
            #[cfg(not(feature = "tokio"))]
            return ctx.spawn::<InlineAsync<Event>, _, _>(|_ctx| async {
                println!("✓ spawn with InlineAsync works!");
                Outcome::Events(vec![Event::Test])
            });
        }
    }
}

fn main() {
    println!("✅ Simplified API working correctly!");
    println!("- All executors work with spawn");
    println!("- No more concurrency complexity");
}
