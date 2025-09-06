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
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::BoxStream;

/// Unified effect output: a single event, a stream of events, or none
pub enum EffectResult<E> {
    Future(BoxFuture<'static, Vec<E>>),
    Stream(BoxStream<'static, E>),
    None,
}

/// Consume an `EffectOutput` by sending events via the provided context
pub async fn consume_effect_output<E, R>(output: EffectResult<E>, ctx: &EffectContext<R>)
where
    E: Send + 'static,
    R: Clone + Send + Sync + 'static,
{
    match output {
        EffectResult::Future(events) => {
            let _ = ctx.send_event(event);
        }
        EffectResult::Stream(mut stream) => {
            while let Some(event) = stream.next().await {
                let _ = ctx.send_event(event);
            }
        }
        EffectResult::None => {}
    }
}
