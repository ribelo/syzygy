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

use crate::effect_context::EffectContext;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::StreamExt;

/// Unified effect output: a single event, a stream of events, or none
pub enum EffectOutput<E> {
    Future(BoxFuture<'static, Vec<E>>),
    Stream(BoxStream<'static, E>),
    None,
}

/// Consume an `EffectOutput` by sending events via the provided context
pub async fn consume_effect_output<E, R>(output: EffectOutput<E>, ctx: &EffectContext<E, R>)
where
    E: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    match output {
        EffectOutput::Future(events_future) => {
            let events = events_future.await;
            for event in events {
                ctx.send_event(event);
            }
        }
        EffectOutput::Stream(mut stream) => {
            while let Some(event) = stream.next().await {
                ctx.send_event(event);
            }
        }
        EffectOutput::None => {}
    }
}

impl<E: std::fmt::Debug> std::fmt::Debug for EffectOutput<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EffectOutput::Future(_) => f.debug_tuple("Future").field(&"<future>").finish(),
            EffectOutput::Stream(_) => f.debug_tuple("Stream").field(&"<stream>").finish(),
            EffectOutput::None => f.debug_tuple("None").finish(),
        }
    }
}
