use crate::command::{Command, CommandStep};
use crate::error::CommandError;
use crossbeam_channel::Sender;

#[cfg(feature = "tracing")]
use tracing::debug;

/// Route a command's outputs to appropriate channels
///
/// This is now a simple synchronous function that iterates over command outputs
/// and routes them to the correct channels. Events go to Core, Effects go to Shell.
pub(crate) fn route_command<Event, Effect>(
    command: Command<Event, Effect>,
    event_sender: &Sender<Event>,
    effect_sender: &Sender<CommandStep<Event, Effect>>,
) -> Result<(), CommandError>
where
    Event: Send + Clone + 'static,
    Effect: Send + Clone + 'static,
{
    #[cfg(feature = "tracing")]
    debug!("Starting synchronous command routing");

    let mut _event_count = 0;
    let mut _effect_count = 0;

    // Simple synchronous iteration over command outputs
    for output in command {
        match output {
            CommandStep::Event(event) => {
                _event_count += 1;
                #[cfg(feature = "tracing")]
                debug!("Routing event to Core");

                event_sender
                    .send(event)
                    .map_err(|_| CommandError::CommandPanic("Event channel closed".to_string()))?;
            }
            CommandStep::Effect(effect) => {
                _effect_count += 1;
                #[cfg(feature = "tracing")]
                debug!("Routing single effect to Shell");

                effect_sender
                    .send(CommandStep::Effect(effect))
                    .map_err(|_| CommandError::CommandPanic("Effect channel closed".to_string()))?;
            }
            CommandStep::Batch(effects) => {
                _effect_count += effects.len();
                #[cfg(feature = "tracing")]
                debug!(count = effects.len(), "Routing batch effects to Shell");

                // Send the entire batch pattern to Shell for proper coordination
                effect_sender
                    .send(CommandStep::Batch(effects))
                    .map_err(|_| CommandError::CommandPanic("Effect channel closed".to_string()))?;
            }
            // No other variants
        }
    }

    #[cfg(feature = "tracing")]
    debug!(
        _event_count,
        _effect_count, "Synchronous command execution completed"
    );

    Ok(())
}

// Deprecated execute_command function removed in favor of route_command

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;

    #[derive(Debug, Clone, PartialEq)]
    enum TestEvent {
        A,
        B,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum TestEffect {
        X,
        Y,
    }

    #[test]
    fn test_execute_empty_command() {
        let (effect_tx, effect_rx) = unbounded::<CommandStep<TestEvent, TestEffect>>();
        let (event_tx, event_rx) = unbounded::<TestEvent>();
        let command = Command::<TestEvent, TestEffect>::none();

        route_command(command, &event_tx, &effect_tx).unwrap();

        // Should have no effects, no events
        assert!(effect_rx.try_recv().is_err());
        assert!(event_rx.try_recv().is_err());
    }

    #[test]
    fn test_execute_event_command() {
        let (event_tx, event_rx) = unbounded::<TestEvent>();
        let (effect_tx, effect_rx) = unbounded::<CommandStep<TestEvent, TestEffect>>();

        let command = Command::event(TestEvent::A);

        route_command(command, &event_tx, &effect_tx).unwrap();

        // Should have one event, no effects
        assert_eq!(event_rx.recv().unwrap(), TestEvent::A);
        assert!(event_rx.try_recv().is_err());
        assert!(effect_rx.try_recv().is_err());
    }

    #[test]
    fn test_execute_effect_command() {
        let (effect_tx, effect_rx) = unbounded::<CommandStep<TestEvent, TestEffect>>();
        let (event_tx, _event_rx) = unbounded::<TestEvent>();

        let command = Command::<TestEvent, TestEffect>::effect(TestEffect::X);

        route_command(command, &event_tx, &effect_tx).unwrap();

        // Should have one effect wrapped in CommandStep
        let cmd_output = effect_rx.recv().unwrap();
        match cmd_output {
            CommandStep::Effect(TestEffect::X) => {
                // Expected single effect
            }
            _ => panic!("Expected CommandStep::Effect(TestEffect::X)"),
        }
        assert!(effect_rx.try_recv().is_err());
    }

    #[test]
    fn test_execute_batch_command() {
        let (event_tx, event_rx) = unbounded::<TestEvent>();
        let (effect_tx, effect_rx) = unbounded::<CommandStep<TestEvent, TestEffect>>();

        let command = Command::batch([
            Command::event(TestEvent::A),
            Command::effect(TestEffect::X),
            Command::event(TestEvent::B),
            Command::effect(TestEffect::Y),
        ]);

        route_command(command, &event_tx, &effect_tx).unwrap();

        // Should have events in order
        assert_eq!(event_rx.recv().unwrap(), TestEvent::A);
        assert_eq!(event_rx.recv().unwrap(), TestEvent::B);
        assert!(event_rx.try_recv().is_err());

        // Should have effects wrapped in CommandStep in order
        let cmd1 = effect_rx.recv().unwrap();
        let cmd2 = effect_rx.recv().unwrap();

        match (cmd1, cmd2) {
            (CommandStep::Effect(TestEffect::X), CommandStep::Effect(TestEffect::Y)) => {
                // Expected effects in order
            }
            _ => panic!("Expected CommandStep::Effect(X) and CommandStep::Effect(Y)"),
        }
        assert!(effect_rx.try_recv().is_err());
    }

    #[test]
    fn test_closed_event_channel_error() {
        let (event_tx, event_rx) = unbounded();
        let (effect_tx, _effect_rx) = unbounded::<CommandStep<TestEvent, TestEffect>>();

        // Drop receiver to close channel
        drop(event_rx);

        let command = Command::event(TestEvent::A);
        let result = route_command(command, &event_tx, &effect_tx);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CommandError::CommandPanic(_)));
    }

    #[test]
    fn test_closed_effect_channel_error() {
        let (effect_tx, effect_rx) = unbounded::<CommandStep<TestEvent, TestEffect>>();
        let (event_tx, _event_rx) = unbounded::<TestEvent>();

        // Drop receiver to close channel
        drop(effect_rx);

        let command = Command::<TestEvent, TestEffect>::effect(TestEffect::X);
        let result = route_command(command, &event_tx, &effect_tx);

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CommandError::CommandPanic(_)));
    }
}
