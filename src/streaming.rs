//! Streaming effect helpers for unified single/stream event handling
//!
//! This module provides lightweight types that match the Streaming Effects
//! proposal: a single unified way to represent effect outputs as either a
//! single event, a stream of events, or no events. A convenience function
//! is provided to drive these outputs using an `EffectContext` by sending
//! events back to the Core.
//!
//! These helpers are entirely optional — the core library remains compatible
//! with handlers that send events directly via `EffectContext::send_event`.

use crate::async_context::EffectContext;
use futures::stream::BoxStream;
use futures::StreamExt;

/// Unified effect output: a single event, a stream of events, or none
pub enum EffectOutput<Event> {
    Single(Event),
    Stream(BoxStream<'static, Event>),
    None,
}

/// Consume an `EffectOutput` by sending events via the provided context
pub async fn consume_effect_output<Event, Resources, Executors>(
    output: EffectOutput<Event>,
    ctx: &EffectContext<Event, Resources, Executors>,
)
where
    Event: Send + 'static,
    Resources: Clone + Send + Sync + 'static,
    Executors: Clone + Send + Sync + 'static,
{
    match output {
        EffectOutput::Single(event) => {
            let _ = ctx.send_event(event);
        }
        EffectOutput::Stream(mut stream) => {
            while let Some(event) = stream.next().await {
                let _ = ctx.send_event(event);
            }
        }
        EffectOutput::None => {}
    }
}
