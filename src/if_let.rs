use std::marker::PhantomData;

use crate::command::Command;
use crate::extract::EventContext;
use crate::reducer::Reducer;

pub struct IfLet<
    Child,
    ParentState,
    ParentEvent,
    ParentEffect,
    StateLens,
    EventFrom,
    EventInto,
    EffectInto,
> {
    child: Child,
    state_lens: StateLens,
    event_from: EventFrom,
    event_into: EventInto,
    effect_into: EffectInto,
    _phantom: PhantomData<(ParentState, ParentEvent, ParentEffect)>,
}

impl<
        Child,
        ParentState,
        ParentEvent,
        ParentEffect,
        StateLens,
        EventFrom,
        EventInto,
        EffectInto,
    >
    IfLet<
        Child,
        ParentState,
        ParentEvent,
        ParentEffect,
        StateLens,
        EventFrom,
        EventInto,
        EffectInto,
    >
{
    #[must_use]
    pub fn new(
        child: Child,
        state_lens: StateLens,
        event_from: EventFrom,
        event_into: EventInto,
        effect_into: EffectInto,
    ) -> Self {
        Self {
            child,
            state_lens,
            event_from,
            event_into,
            effect_into,
            _phantom: PhantomData,
        }
    }
}

impl<
        ParentState,
        ParentEvent,
        ParentEffect,
        Child,
        ChildState,
        ChildEvent,
        ChildEffect,
        StateLens,
        EventFrom,
        EventInto,
        EffectInto,
    > Reducer
    for IfLet<
        Child,
        ParentState,
        ParentEvent,
        ParentEffect,
        StateLens,
        EventFrom,
        EventInto,
        EffectInto,
    >
where
    Child: Reducer<State = ChildState, Event = ChildEvent, Effect = ChildEffect>,
    StateLens: Fn(&mut ParentState) -> &mut Option<ChildState>,
    EventFrom: Fn(ParentEvent) -> ChildEvent,
    EventInto: Fn(ChildEvent) -> ParentEvent,
    EffectInto: Fn(ChildEffect) -> ParentEffect,
{
    type State = ParentState;
    type Event = ParentEvent;
    type Effect = ParentEffect;

    fn reduce(
        &self,
        event: Self::Event,
        ctx: &EventContext<Self::State>,
    ) -> Command<Self::Event, Self::Effect> {
        let has_child = {
            // SAFETY: EventContext points to the active model for this dispatch.
            let parent_state = unsafe { &mut *ctx.model_ptr() };
            (self.state_lens)(parent_state).is_some()
        };

        if !has_child {
            return Command::none();
        }

        let child_event = (self.event_from)(event);
        let child_command = {
            // SAFETY: EventContext points to the active model for this dispatch.
            let model = unsafe { &mut *ctx.model_ptr() };
            let Some(child_state) = (self.state_lens)(model).as_mut() else {
                unreachable!("child state must be present after if_let guard")
            };
            let child_ctx = EventContext::new(child_state);
            self.child.reduce(child_event, &child_ctx)
        };

        child_command
            .map_event(|child_event| (self.event_into)(child_event))
            .map_effect(|child_effect| (self.effect_into)(child_effect))
    }
}

#[must_use]
pub fn if_let<
    Child,
    ParentState,
    ParentEvent,
    ParentEffect,
    ChildState,
    ChildEvent,
    ChildEffect,
    StateLens,
    EventFrom,
    EventInto,
    EffectInto,
>(
    child: Child,
    state_lens: StateLens,
    event_from: EventFrom,
    event_into: EventInto,
    effect_into: EffectInto,
) -> IfLet<Child, ParentState, ParentEvent, ParentEffect, StateLens, EventFrom, EventInto, EffectInto>
where
    Child: Reducer<State = ChildState, Event = ChildEvent, Effect = ChildEffect>,
    StateLens: Fn(&mut ParentState) -> &mut Option<ChildState>,
    EventFrom: Fn(ParentEvent) -> ChildEvent,
    EventInto: Fn(ChildEvent) -> ParentEvent,
    EffectInto: Fn(ChildEffect) -> ParentEffect,
{
    IfLet::new(child, state_lens, event_from, event_into, effect_into)
}

#[cfg(test)]
mod tests {
    use super::if_let;
    use crate::command::{Command, CommandStep};
    use crate::extract::EventContext;
    use crate::reducer::{Reduce, Reducer};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ChildEvent {
        Increment(i32),
        Emit,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ChildEffect {
        Emitted,
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct ChildState {
        value: i32,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ParentEvent {
        Child(ChildEvent),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ParentEffect {
        Child(ChildEffect),
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct ParentState {
        child: Option<ChildState>,
    }

    fn child_reducer() -> Reduce<ChildState, ChildEvent, ChildEffect> {
        Reduce::new(
            |event: ChildEvent, ctx: &EventContext<ChildState>| match event {
                ChildEvent::Increment(amount) => {
                    // SAFETY: EventContext points to a live ChildState for this dispatch.
                    let state = unsafe { &mut *ctx.model_ptr() };
                    state.value += amount;
                    Command::none()
                }
                ChildEvent::Emit => {
                    Command::event(ChildEvent::Increment(1)).and_effect(ChildEffect::Emitted)
                }
            },
        )
    }

    #[test]
    fn if_let_runs_child_when_some() {
        let reducer = if_let(
            child_reducer(),
            |state: &mut ParentState| &mut state.child,
            |event: ParentEvent| match event {
                ParentEvent::Child(child_event) => child_event,
            },
            ParentEvent::Child,
            ParentEffect::Child,
        );

        let mut state = ParentState {
            child: Some(ChildState::default()),
        };
        let ctx = EventContext::new(&mut state);

        let command = reducer.reduce(ParentEvent::Child(ChildEvent::Increment(3)), &ctx);

        assert!(command.is_empty());
        assert_eq!(state.child.as_ref().map(|child| child.value), Some(3));
    }

    #[test]
    fn if_let_returns_none_when_none() {
        let reducer = if_let(
            child_reducer(),
            |state: &mut ParentState| &mut state.child,
            |event: ParentEvent| match event {
                ParentEvent::Child(child_event) => child_event,
            },
            ParentEvent::Child,
            ParentEffect::Child,
        );

        let mut state = ParentState { child: None };
        let ctx = EventContext::new(&mut state);

        let command = reducer.reduce(ParentEvent::Child(ChildEvent::Increment(3)), &ctx);

        assert!(command.is_empty());
    }

    #[test]
    fn if_let_maps_commands_to_parent_types() {
        let reducer = if_let(
            child_reducer(),
            |state: &mut ParentState| &mut state.child,
            |event: ParentEvent| match event {
                ParentEvent::Child(child_event) => child_event,
            },
            ParentEvent::Child,
            ParentEffect::Child,
        );

        let mut state = ParentState {
            child: Some(ChildState::default()),
        };
        let ctx = EventContext::new(&mut state);

        let command = reducer.reduce(ParentEvent::Child(ChildEvent::Emit), &ctx);

        assert_eq!(
            command.into_iter().collect::<Vec<_>>(),
            vec![
                CommandStep::Event(ParentEvent::Child(ChildEvent::Increment(1))),
                CommandStep::Effect(ParentEffect::Child(ChildEffect::Emitted)),
            ]
        );
    }

    #[test]
    fn if_let_does_not_panic_on_none() {
        let reducer = if_let(
            Reduce::new(|_event: ChildEvent, _ctx: &EventContext<ChildState>| {
                panic!("child reducer must not run when optional state is None");
            }),
            |state: &mut ParentState| &mut state.child,
            |event: ParentEvent| match event {
                ParentEvent::Child(child_event) => child_event,
            },
            ParentEvent::Child,
            ParentEffect::Child,
        );

        let mut state = ParentState { child: None };
        let ctx = EventContext::new(&mut state);

        let command = reducer.reduce(ParentEvent::Child(ChildEvent::Increment(9)), &ctx);

        assert!(command.is_empty());
    }
}
