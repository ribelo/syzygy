#![allow(clippy::enum_variant_names)]

use std::time::Duration;
use syzygy::prelude::*;

#[derive(Debug, Default, Model)]
struct AppModel {
    data: Option<String>,
    is_loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    FetchData,
    FetchSuccess(String),
    FetchError(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {
    MakeHttpRequest,
}

/// ## Why use loading states?
///
/// While the async operation occurs, our pure UI state needs to reflect the pending status.
/// We update `is_loading` synchronously before returning the asynchronous effect.
fn fetch_data(is_loading: &mut IsLoading) -> Command<AppEvent, AppEffect> {
    **is_loading = true;
    Command::effect(AppEffect::MakeHttpRequest)
}

fn fetch_success(
    payload: String,
    data: &mut Data,
    is_loading: &mut IsLoading,
) -> Command<AppEvent, AppEffect> {
    **data = Some(payload);
    **is_loading = false;
    Command::none()
}

fn fetch_error(_err: String, is_loading: &mut IsLoading) -> Command<AppEvent, AppEffect> {
    **is_loading = false;
    Command::none()
}

fn handle_event(event: AppEvent, ctx: &EventContext<AppModel>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::FetchData => handle!(fetch_data, ctx),
        AppEvent::FetchSuccess(payload) => handle!(fetch_success, ctx, payload),
        AppEvent::FetchError(err) => handle!(fetch_error, ctx, err),
    }
}

/// ## Why use async functions for effects?
///
/// Async functions that return `Command` are automatically inferred as `Task::future`
/// by the `EffectHandler` macro implementation. This allows you to write natural
/// asynchronous code inside effect handlers.
///
/// Handlers must return a `Command` containing the events to trigger after the
/// async operation completes.
async fn make_http_request((): ()) -> Command<AppEvent, AppEffect> {
    // Simulate a network delay using the configured async runtime
    syzygy::runtime::sleep(Duration::from_millis(10)).await;

    // Simulate a successful response
    // (In a real app, you would match on `Result` and map `Ok`/`Err` to events)
    let success = true;
    if success {
        Command::event(AppEvent::FetchSuccess("Hello from server!".to_string()))
    } else {
        Command::event(AppEvent::FetchError("Request failed".to_string()))
    }
}

fn handle_effect(effect: AppEffect, ctx: &EffectContext<'_>) -> Task<AppEvent, AppEffect> {
    match effect {
        // `handle!` on an `async fn` automatically constructs a `Task::Future`
        AppEffect::MakeHttpRequest => handle!(make_http_request, ctx),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = Syzygy::builder::<AppEvent, AppEffect>()
        .model(AppModel::default())
        .event_handler(handle_event)
        .effect_handler(handle_effect)
        .build();

    app.core().try_send(AppEvent::FetchData)?;

    // Syzygy owns progression synchronously; effects remain async internally.
    app.run_until(|core, _| core.model().data.is_some())?;

    println!("Data: {:?}", app.model().data);
    Ok(())
}
