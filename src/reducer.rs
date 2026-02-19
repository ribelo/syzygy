use std::marker::PhantomData;

use crate::command::Command;
use crate::extract::EventContext;

pub trait Reducer {
    type State;
    type Event;
    type Effect;

    fn reduce(
        &self,
        event: Self::Event,
        ctx: &EventContext<Self::State>,
    ) -> Command<Self::Event, Self::Effect>;

    #[must_use]
    fn scope<ParentState, ParentEvent, ParentEffect, StateLens, EventFrom, EventInto, EffectInto>(
        self,
        state_lens: StateLens,
        event_from: EventFrom,
        event_into: EventInto,
        effect_into: EffectInto,
    ) -> Scope<
        Self,
        ParentState,
        ParentEvent,
        ParentEffect,
        StateLens,
        EventFrom,
        EventInto,
        EffectInto,
    >
    where
        Self: Sized,
        StateLens: Fn(&mut ParentState) -> &mut Self::State,
        EventFrom: Fn(ParentEvent) -> Option<Self::Event>,
        EventInto: Fn(Self::Event) -> ParentEvent,
        EffectInto: Fn(Self::Effect) -> ParentEffect,
    {
        Scope::new(self, state_lens, event_from, event_into, effect_into)
    }
}

type ReduceFn<S, E, X> = dyn Fn(E, &EventContext<S>) -> Command<E, X> + Send;

pub struct Reduce<S, E, X> {
    f: Box<ReduceFn<S, E, X>>,
}

impl<S, E, X> Reduce<S, E, X> {
    #[must_use]
    pub fn new(f: impl Fn(E, &EventContext<S>) -> Command<E, X> + Send + 'static) -> Self {
        Self { f: Box::new(f) }
    }
}

impl<S, E, X> Reducer for Reduce<S, E, X> {
    type State = S;
    type Event = E;
    type Effect = X;

    fn reduce(&self, event: Self::Event, ctx: &EventContext<Self::State>) -> Command<E, X> {
        (self.f)(event, ctx)
    }
}

pub struct Scope<
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
    _marker: PhantomData<fn(ParentState, ParentEvent, ParentEffect)>,
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
    Scope<
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
            _marker: PhantomData,
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
    for Scope<
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
    StateLens: Fn(&mut ParentState) -> &mut ChildState,
    EventFrom: Fn(ParentEvent) -> Option<ChildEvent>,
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
        if let Some(child_event) = (self.event_from)(event) {
            let child_ctx = ctx.scope(|model| (self.state_lens)(model));
            self.child
                .reduce(child_event, &child_ctx)
                .map_event(|event| (self.event_into)(event))
                .map_effect(|effect| (self.effect_into)(effect))
        } else {
            Command::none()
        }
    }
}

pub type BoxedReducer<S, E, X> = Box<dyn Reducer<State = S, Event = E, Effect = X> + Send>;

pub trait ReducerExt: Reducer + Sized {
    #[must_use]
    fn boxed(self) -> BoxedReducer<Self::State, Self::Event, Self::Effect>
    where
        Self: Send + 'static,
    {
        Box::new(self)
    }
}

impl<T: Reducer> ReducerExt for T {}

pub struct Combine<S, E, X> {
    reducers: Vec<BoxedReducer<S, E, X>>,
}

impl<S, E, X> Default for Combine<S, E, X> {
    fn default() -> Self {
        Self {
            reducers: Vec::new(),
        }
    }
}

impl<S, E, X> Combine<S, E, X> {
    #[must_use]
    pub fn new(reducers: Vec<BoxedReducer<S, E, X>>) -> Self {
        Self { reducers }
    }

    pub fn push<R>(&mut self, reducer: R)
    where
        R: Reducer<State = S, Event = E, Effect = X> + Send + 'static,
    {
        self.reducers.push(Box::new(reducer));
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.reducers.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.reducers.is_empty()
    }
}

impl<S, E, X> Reducer for Combine<S, E, X>
where
    E: Clone,
{
    type State = S;
    type Event = E;
    type Effect = X;

    fn reduce(
        &self,
        event: Self::Event,
        ctx: &EventContext<Self::State>,
    ) -> Command<Self::Event, Self::Effect> {
        let Some((last, rest)) = self.reducers.split_last() else {
            return Command::none();
        };

        let mut commands = Vec::with_capacity(self.reducers.len());
        for reducer in rest {
            commands.push(reducer.reduce(event.clone(), ctx));
        }
        commands.push(last.reduce(event, ctx));

        Command::batch(commands)
    }
}

#[must_use]
pub fn combine<S, E, X>(reducers: Vec<BoxedReducer<S, E, X>>) -> Combine<S, E, X>
where
    E: Clone,
{
    Combine::new(reducers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandStep;
    use crate::test_store::TestStore;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum CounterEvent {
        Increment(i32),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum CounterEffect {
        Changed(i32),
    }

    #[derive(Debug, Default)]
    struct CounterState {
        value: i32,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum AppEvent {
        Counter(CounterEvent),
        Ping,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum AppEffect {
        Counter(CounterEffect),
        Audit(&'static str),
        Secondary(&'static str),
    }

    #[derive(Debug, Default)]
    struct AppState {
        counter: CounterState,
    }

    #[test]
    fn reduce_wraps_closure() {
        let reducer =
            Reduce::new(
                |event: CounterEvent, _ctx: &EventContext<CounterState>| match event {
                    CounterEvent::Increment(amount) => {
                        Command::effect(CounterEffect::Changed(amount))
                    }
                },
            );

        let mut state = CounterState::default();
        let ctx = EventContext::new(&mut state);

        let command = reducer.reduce(CounterEvent::Increment(3), &ctx);

        assert_eq!(
            command.into_iter().collect::<Vec<_>>(),
            vec![CommandStep::Effect(CounterEffect::Changed(3))]
        );
    }

    #[test]
    fn scope_adapts_child_reducer_to_parent_types() {
        let child = Reduce::new(
            |event: CounterEvent, _ctx: &EventContext<CounterState>| match event {
                CounterEvent::Increment(amount) => Command::event(CounterEvent::Increment(amount))
                    .and_effect(CounterEffect::Changed(amount)),
            },
        );

        let parent = child.scope(
            |state: &mut AppState| &mut state.counter,
            |event: AppEvent| match event {
                AppEvent::Counter(event) => Some(event),
                AppEvent::Ping => None,
            },
            AppEvent::Counter,
            AppEffect::Counter,
        );

        let mut state = AppState::default();
        let ctx = EventContext::new(&mut state);

        let mapped = parent.reduce(AppEvent::Counter(CounterEvent::Increment(2)), &ctx);
        assert_eq!(
            mapped.into_iter().collect::<Vec<_>>(),
            vec![
                CommandStep::Event(AppEvent::Counter(CounterEvent::Increment(2))),
                CommandStep::Effect(AppEffect::Counter(CounterEffect::Changed(2))),
            ]
        );

        let no_match = parent.reduce(AppEvent::Ping, &ctx);
        assert!(no_match.is_empty());
    }

    #[test]
    fn combine_runs_all_reducers() {
        let first = Reduce::new(
            |event: AppEvent, _ctx: &EventContext<AppState>| match event {
                AppEvent::Ping => Command::effect(AppEffect::Audit("first")),
                AppEvent::Counter(_) => Command::none(),
            },
        );
        let second = Reduce::new(
            |event: AppEvent, _ctx: &EventContext<AppState>| match event {
                AppEvent::Ping => Command::effect(AppEffect::Secondary("second")),
                AppEvent::Counter(_) => Command::none(),
            },
        );

        let reducer = combine(vec![first.boxed(), second.boxed()]);

        let mut state = AppState::default();
        let ctx = EventContext::new(&mut state);

        let command = reducer.reduce(AppEvent::Ping, &ctx);

        assert_eq!(
            command.into_iter().collect::<Vec<_>>(),
            vec![
                CommandStep::Effect(AppEffect::Audit("first")),
                CommandStep::Effect(AppEffect::Secondary("second")),
            ]
        );
    }

    #[test]
    fn reducer_works_with_test_store() {
        let counter = Reduce::new(|event: CounterEvent, ctx: &EventContext<CounterState>| {
            match event {
                CounterEvent::Increment(amount) => {
                    // SAFETY: EventContext points to a live CounterState for this dispatch.
                    let state = unsafe { &mut *ctx.model_ptr() };
                    state.value += amount;
                    Command::effect(CounterEffect::Changed(state.value))
                }
            }
        });

        let reducer = counter.scope(
            |state: &mut AppState| &mut state.counter,
            |event: AppEvent| match event {
                AppEvent::Counter(event) => Some(event),
                AppEvent::Ping => None,
            },
            AppEvent::Counter,
            AppEffect::Counter,
        );

        let mut store = TestStore::new(AppState::default(), move |event, ctx| {
            reducer.reduce(event, ctx)
        });

        store.send(AppEvent::Counter(CounterEvent::Increment(4)));

        assert_eq!(store.state().counter.value, 4);
        store.assert_effects([AppEffect::Counter(CounterEffect::Changed(4))]);
    }

    #[cfg(feature = "shell")]
    #[test]
    fn builder_accepts_reducer() {
        use crate::executor::Task;
        use crate::extract::EffectContext;
        use crate::syzygy::Syzygy;

        let counter = Reduce::new(|event: CounterEvent, ctx: &EventContext<CounterState>| {
            match event {
                CounterEvent::Increment(amount) => {
                    // SAFETY: EventContext points to a live CounterState for this dispatch.
                    let state = unsafe { &mut *ctx.model_ptr() };
                    state.value += amount;
                    Command::effect(CounterEffect::Changed(state.value))
                }
            }
        });

        let reducer = counter.scope(
            |state: &mut AppState| &mut state.counter,
            |event: AppEvent| match event {
                AppEvent::Counter(event) => Some(event),
                AppEvent::Ping => None,
            },
            AppEvent::Counter,
            AppEffect::Counter,
        );

        let mut runner = Syzygy::builder::<AppEvent, AppEffect>()
            .model(AppState::default())
            .reducer(reducer)
            .effect_handler(|_effect: AppEffect, _ctx: &EffectContext<()>| {
                Task::<AppEvent, AppEffect>::none()
            })
            .build();

        runner
            .core()
            .try_send_event(AppEvent::Counter(CounterEvent::Increment(6)))
            .expect("event should enqueue");

        runner.step().expect("step should complete");

        assert_eq!(runner.model().counter.value, 6);
    }
}
