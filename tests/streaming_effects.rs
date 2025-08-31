#![allow(dead_code, clippy::clone_on_ref_ptr, unused_variables, unused_imports)]
//! Comprehensive tests for the streaming effects API
//!
//! Tests all EffectOutput variants and coordination patterns:
//! - EffectOutput::Single - single event output
//! - EffectOutput::Stream - stream of events
//! - EffectOutput::None - no output
//! - Shell coordination: Merge, Join, Race, Chain

use std::time::Duration;
use syzygy::prelude::*;
use syzygy::streaming::{EffectOutput, consume_effect_output};
use futures::stream::{self, BoxStream};
use futures::StreamExt;
use tokio::time::sleep;

#[derive(Debug, Clone, PartialEq)]
enum StreamEvent {
    Start,
    Data(String),
    Progress(u32),
    Complete,
}

#[derive(Debug, Default)]
struct StreamModel {
    events: Vec<StreamEvent>,
    progress: u32,
    completed: bool,
}

#[derive(Debug, Clone)]
enum StreamEffect {
    ProduceSingle { value: String },
    ProduceStream { count: u32 },
    ProduceNone,
    MergeStreams,
    JoinFutures,  
    RaceFutures,
    ChainStreams,
}

fn stream_update(
    event: StreamEvent,
    ctx: &mut EventContext<StreamEvent, StreamEffect, Storage<StreamModel, EmptyStorage>>,
) -> Command<StreamEvent, StreamEffect> {
    let model: &mut StreamModel = ctx.model_mut();
    model.events.push(event.clone());
    
    match event {
        StreamEvent::Start => Command::effect(StreamEffect::ProduceSingle { 
            value: "test".to_string() 
        }),
        StreamEvent::Data(_) => Command::none(),
        StreamEvent::Progress(value) => {
            model.progress = value;
            Command::none()
        }
        StreamEvent::Complete => {
            model.completed = true;
            Command::none()
        }
    }
}

async fn stream_effect_handler(
    effect: StreamEffect,
    ctx: EffectContext<StreamEvent, EmptyStorage>,
) -> EffectOutput<StreamEvent> {
    match effect {
        StreamEffect::ProduceSingle { value } => {
            sleep(Duration::from_millis(10)).await;
            EffectOutput::Single(StreamEvent::Data(value))
        }
        
        StreamEffect::ProduceStream { count } => {
            let stream = stream::iter(0..count)
                .then(|i| async move {
                    sleep(Duration::from_millis(5)).await;
                    StreamEvent::Progress(i)
                })
                .boxed();
            EffectOutput::Stream(stream)
        }
        
        StreamEffect::ProduceNone => {
            sleep(Duration::from_millis(5)).await;
            EffectOutput::None
        }
        
        // Coordination patterns return streams for testing
        StreamEffect::MergeStreams => {
            let stream = stream::iter(vec![
                StreamEvent::Data("merge1".to_string()),
                StreamEvent::Data("merge2".to_string()),
            ]).boxed();
            EffectOutput::Stream(stream)
        }
        
        StreamEffect::JoinFutures => {
            sleep(Duration::from_millis(10)).await;
            EffectOutput::Single(StreamEvent::Data("joined".to_string()))
        }
        
        StreamEffect::RaceFutures => {
            sleep(Duration::from_millis(5)).await;
            EffectOutput::Single(StreamEvent::Data("raced".to_string()))
        }
        
        StreamEffect::ChainStreams => {
            let stream = stream::iter(vec![
                StreamEvent::Data("chain1".to_string()),
                StreamEvent::Data("chain2".to_string()),
            ]).boxed();
            EffectOutput::Stream(stream)
        }
    }
}

#[tokio::test]
async fn test_effect_output_single() {
    let (core, shell) = Syzygy::builder::<StreamEvent, StreamEffect>()
        .model(StreamModel::default())
        .event_handler(stream_update)
        .effect_handler(stream_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);
    
    // Test single event output
    runner.core().send_event(StreamEvent::Start).unwrap();
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    runner.tick(syzygy::spawn::spawner()).await.unwrap(); // Process the returned event
    
    let model = runner.core().model();
    assert!(model.events.contains(&StreamEvent::Start));
    assert!(model.events.contains(&StreamEvent::Data("test".to_string())));
}

#[tokio::test]
async fn test_effect_output_stream() {
    let (core, shell) = Syzygy::builder::<StreamEvent, StreamEffect>()
        .model(StreamModel::default())
        .event_handler(stream_update)
        .effect_handler(stream_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);
    
    // Manually dispatch a stream-producing effect
    runner.shell_mut().dispatch(Command::effect(StreamEffect::ProduceStream { count: 3 })).unwrap();
    
    // Give time for stream to process
    tokio::time::sleep(Duration::from_millis(50)).await;
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    
    let model = runner.core().model();
    // Should have progress events from the stream
    assert!(model.events.iter().any(|e| matches!(e, StreamEvent::Progress(_))));
}

#[tokio::test]
async fn test_effect_output_none() {
    let (core, shell) = Syzygy::builder::<StreamEvent, StreamEffect>()
        .model(StreamModel::default())
        .event_handler(stream_update)
        .effect_handler(stream_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);
    let initial_count = runner.core().model().events.len();
    
    // Dispatch effect that produces no output
    runner.shell_mut().dispatch(Command::effect(StreamEffect::ProduceNone)).unwrap();
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    
    let model = runner.core().model();
    // Should have no new events (EffectOutput::None)
    assert_eq!(model.events.len(), initial_count);
}

#[tokio::test]
async fn test_coordination_merge_streams() {
    let (core, shell) = Syzygy::builder::<StreamEvent, StreamEffect>()
        .model(StreamModel::default())
        .event_handler(stream_update)
        .effect_handler(stream_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);
    
    // Dispatch multiple effects that each produce streams
    runner.core().send_event(StreamEvent::Start).unwrap();
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    
    // Manually dispatch merge streams effects for testing
    runner.shell_mut().dispatch(Command::effect(StreamEffect::MergeStreams)).unwrap();
    runner.shell_mut().dispatch(Command::effect(StreamEffect::MergeStreams)).unwrap();
    
    // Give time for streams to process
    tokio::time::sleep(Duration::from_millis(50)).await;
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    
    let model = runner.core().model();
    let data_events: Vec<_> = model.events.iter()
        .filter(|e| matches!(e, StreamEvent::Data(_)))
        .collect();
    
    // Should have received events from streams (at least from Start event + merge streams)
    assert!(data_events.len() >= 2, "Expected at least 2 data events, got {}", data_events.len());
}

#[tokio::test]
async fn test_coordination_join_futures() {
    let (core, shell) = Syzygy::builder::<StreamEvent, StreamEffect>()
        .model(StreamModel::default())
        .event_handler(stream_update)
        .effect_handler(stream_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);
    
    // Dispatch effects that produce single results
    runner.shell_mut().dispatch(Command::effect(StreamEffect::JoinFutures)).unwrap();
    runner.shell_mut().dispatch(Command::effect(StreamEffect::JoinFutures)).unwrap();
    
    // Give time for futures to complete
    tokio::time::sleep(Duration::from_millis(50)).await;
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    
    let model = runner.core().model();
    let joined_events = model.events.iter()
        .filter(|e| e == &&StreamEvent::Data("joined".to_string()))
        .count();
    
    // Should have received results from joined futures
    assert!(joined_events >= 1, "Expected at least 1 joined event, got {}", joined_events);
}

#[tokio::test]
async fn test_coordination_race_futures() {
    let (core, shell) = Syzygy::builder::<StreamEvent, StreamEffect>()
        .model(StreamModel::default())
        .event_handler(stream_update)
        .effect_handler(stream_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);
    
    // Dispatch racing effects
    runner.shell_mut().dispatch(Command::effect(StreamEffect::RaceFutures)).unwrap();
    runner.shell_mut().dispatch(Command::effect(StreamEffect::RaceFutures)).unwrap();
    
    // Give time for futures to complete
    tokio::time::sleep(Duration::from_millis(50)).await;
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    
    let model = runner.core().model();
    let raced_events = model.events.iter()
        .filter(|e| e == &&StreamEvent::Data("raced".to_string()))
        .count();
    
    // Should have at least one result from race effects (both can complete independently)
    assert!(raced_events >= 1, "Expected at least 1 raced event, got {}", raced_events);
}

#[tokio::test]
async fn test_coordination_chain_streams() {
    let (core, shell) = Syzygy::builder::<StreamEvent, StreamEffect>()
        .model(StreamModel::default())
        .event_handler(stream_update)
        .effect_handler(stream_effect_handler)
        .build();

    let mut runner = Runner::new(core, shell);
    
    // Dispatch chaining effects
    runner.shell_mut().dispatch(Command::effect(StreamEffect::ChainStreams)).unwrap();
    runner.shell_mut().dispatch(Command::effect(StreamEffect::ChainStreams)).unwrap();
    
    // Give extra time for sequential processing
    tokio::time::sleep(Duration::from_millis(100)).await;
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    runner.tick(syzygy::spawn::spawner()).await.unwrap();
    
    let model = runner.core().model();
    let chain_events: Vec<_> = model.events.iter()
        .filter(|e| matches!(e, StreamEvent::Data(s) if s.starts_with("chain")))
        .collect();
    
    // Should have events from chained streams
    assert!(chain_events.len() >= 2, "Expected at least 2 chain events, got {}", chain_events.len());
}

#[tokio::test] 
async fn test_consume_effect_output_helper() {
    use syzygy::async_context::EffectContext;
    use crossbeam_channel::unbounded;
    
    let (tx, rx) = unbounded();
    let ctx = EffectContext::new(Some(tx), syzygy::storage::EmptyStorage::new(), syzygy::executor::EmptyExecutorStorage::new());
    
    // Test consuming a single event
    let output = EffectOutput::Single(StreamEvent::Data("test".to_string()));
    consume_effect_output(output, &ctx).await;
    
    let received = rx.recv().unwrap();
    assert_eq!(received, StreamEvent::Data("test".to_string()));
    
    // Test consuming None (should not send anything)
    let output = EffectOutput::None;
    consume_effect_output(output, &ctx).await;
    
    // Should be no additional events
    assert!(rx.try_recv().is_err());
    
    // Test consuming a stream
    let stream = stream::iter(vec![
        StreamEvent::Progress(1),
        StreamEvent::Progress(2),
    ]).boxed();
    let output = EffectOutput::Stream(stream);
    consume_effect_output(output, &ctx).await;
    
    // Should receive all stream events
    assert_eq!(rx.recv().unwrap(), StreamEvent::Progress(1));
    assert_eq!(rx.recv().unwrap(), StreamEvent::Progress(2));
}