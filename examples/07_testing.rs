#![cfg_attr(not(test), allow(dead_code))]

use syzygy::prelude::*;

#[derive(Debug, Default, Model, Clone, PartialEq)]
struct AppModel {
    counter: i32,
    loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEvent {
    Increment,
    Save,
    SaveCompleted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AppEffect {
    SaveToServer(i32),
}

fn increment(_: (), counter: &mut Counter) -> Command<AppEvent, AppEffect> {
    **counter += 1;
    Command::none()
}

fn save(_: (), counter: &mut Counter, loading: &mut Loading) -> Command<AppEvent, AppEffect> {
    **loading = true;
    Command::effect(AppEffect::SaveToServer(**counter))
}

fn save_completed(_: (), loading: &mut Loading) -> Command<AppEvent, AppEffect> {
    **loading = false;
    Command::none()
}

fn handle_event(event: AppEvent, ctx: &EventContext<AppModel>) -> Command<AppEvent, AppEffect> {
    match event {
        AppEvent::Increment => handle!(increment, ctx),
        AppEvent::Save => handle!(save, ctx),
        AppEvent::SaveCompleted => handle!(save_completed, ctx),
    }
}

/// ## Why use a TestStore?
///
/// `TestStore` provides a synchronous, deterministic environment for
/// testing TEA components. It captures state changes and emitted effects.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pure_state_update() {
        let mut store = TestStore::new(AppModel::default(), handle_event);

        // ## Why `send`?
        // Sending an event synchronously executes the event handler.
        store.send(AppEvent::Increment);

        // ## Why `assert_state`?
        // It compares the final model state to the expected state.
        store.assert_state(&AppModel {
            counter: 1,
            loading: false,
        });

        // ## Why `assert_no_effects`?
        // It verifies that no side effects were accidentally emitted.
        store.assert_no_effects();
    }

    #[test]
    fn effect_emission() {
        let mut store = TestStore::new(
            AppModel {
                counter: 5,
                loading: false,
            },
            handle_event,
        );

        store.send(AppEvent::Save);

        // Assert the state changed appropriately
        assert!(store.state().loading);

        // ## Why `assert_effects`?
        // Tests that specific effects were emitted. This also consumes the
        // effects from the store's buffer.
        store.assert_effects([AppEffect::SaveToServer(5)]);
    }

    #[test]
    fn async_receive_pattern() {
        syzygy::runtime::block_on(async {
            let mut store = TestStore::new(AppModel::default(), handle_event);

            store.send(AppEvent::Save);

            // ## Why `receive_async`?
            // This executes the effect handler strictly for testing and feeds
            // the resulting events right back into the TestStore loop!
            store
                .receive_async(|effect, _ctx| match effect {
                    AppEffect::SaveToServer(_) => {
                        Task::once(async { Command::event(AppEvent::SaveCompleted) })
                    }
                })
                .await;

            assert!(!store.state().loading);
            store.assert_no_effects();
        });
    }
}

fn main() {
    println!("Run 'cargo test --example 07_testing' to see the tests!");
}
