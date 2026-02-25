use std::marker::PhantomData;
use std::sync::Arc;

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
}

type ReduceFn<S, E, X> = dyn Fn(E, &EventContext<S>) -> Command<E, X>;

pub struct Reduce<S, E, X> {
    f: Box<ReduceFn<S, E, X>>,
}

impl<S, E, X> Reduce<S, E, X> {
    #[must_use]
    pub fn new(f: impl Fn(E, &EventContext<S>) -> Command<E, X> + 'static) -> Self {
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

pub struct Combined<R1, R2> {
    r1: R1,
    r2: R2,
}

impl<R1, R2> Combined<R1, R2> {
    #[must_use]
    pub fn new(r1: R1, r2: R2) -> Self {
        Self { r1, r2 }
    }
}

impl<S, E, X, R1, R2> Reducer for Combined<R1, R2>
where
    E: Clone,
    R1: Reducer<State = S, Event = E, Effect = X>,
    R2: Reducer<State = S, Event = E, Effect = X>,
{
    type State = S;
    type Event = E;
    type Effect = X;

    fn reduce(
        &self,
        event: Self::Event,
        ctx: &EventContext<Self::State>,
    ) -> Command<Self::Event, Self::Effect> {
        let cmd1 = self.r1.reduce(event.clone(), ctx);
        let cmd2 = self.r2.reduce(event, ctx);
        cmd1.and(cmd2)
    }
}

type DebugPrinter = dyn Fn(&str) + Send + Sync;

pub struct DebugReducer<R> {
    inner: R,
    printer: Option<Arc<DebugPrinter>>,
}

pub struct OnChange<R, V, F, ReactFn> {
    inner: R,
    selector: F,
    reaction: ReactFn,
    _phantom: PhantomData<V>,
}

impl<R, V, F, ReactFn> OnChange<R, V, F, ReactFn> {
    #[must_use]
    pub fn new(inner: R, selector: F, reaction: ReactFn) -> Self {
        Self {
            inner,
            selector,
            reaction,
            _phantom: PhantomData,
        }
    }
}

impl<R> DebugReducer<R> {
    #[must_use]
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            printer: None,
        }
    }

    #[must_use]
    pub fn with_printer<P>(inner: R, printer: P) -> Self
    where
        P: Fn(&str) + Send + Sync + 'static,
    {
        Self {
            inner,
            printer: Some(Arc::new(printer)),
        }
    }

    #[cfg(debug_assertions)]
    fn print(&self, message: &str) {
        if let Some(printer) = &self.printer {
            (printer)(message);
        } else {
            eprintln!("{message}");
        }
    }
}

impl<R> DebugReducer<R>
where
    R: Reducer,
    R::State: std::fmt::Debug,
{
    #[cfg(debug_assertions)]
    fn print_changes(&self, event_repr: &str, old_state: &R::State, new_state: &R::State) {
        let old = format!("{old_state:?}");
        let new = format!("{new_state:?}");

        if old == new {
            self.print(&format!(
                "received event: {event_repr}\n  (no state changes)"
            ));
            return;
        }

        self.print(&format!(
            "received event: {event_repr}\n  state changed:\n    old: {old}\n    new: {new}"
        ));
    }
}

pub type BoxedReducer<S, E, X> = Box<dyn Reducer<State = S, Event = E, Effect = X>>;

pub trait ReducerExt: Reducer + Sized {
    #[must_use]
    fn boxed(self) -> BoxedReducer<Self::State, Self::Event, Self::Effect>
    where
        Self: 'static,
    {
        Box::new(self)
    }

    #[must_use]
    fn debug(self) -> DebugReducer<Self>
    where
        Self::State: Clone + std::fmt::Debug,
        Self::Event: std::fmt::Debug,
    {
        DebugReducer::new(self)
    }

    #[must_use]
    fn debug_with<P>(self, printer: P) -> DebugReducer<Self>
    where
        Self::State: Clone + std::fmt::Debug,
        Self::Event: std::fmt::Debug,
        P: Fn(&str) + Send + Sync + 'static,
    {
        DebugReducer::with_printer(self, printer)
    }

    #[must_use]
    fn on_change<V, F, ReactFn>(
        self,
        selector: F,
        reaction: ReactFn,
    ) -> OnChange<Self, V, F, ReactFn>
    where
        V: PartialEq + Clone,
        F: Fn(&Self::State) -> V + 'static,
        ReactFn:
            Fn(V, V, &EventContext<Self::State>) -> Command<Self::Event, Self::Effect> + 'static,
    {
        OnChange::new(self, selector, reaction)
    }
}

impl<T: Reducer> ReducerExt for T {}

impl<R, V, F, ReactFn> Reducer for OnChange<R, V, F, ReactFn>
where
    R: Reducer,
    V: PartialEq + Clone,
    F: Fn(&R::State) -> V,
    ReactFn: Fn(V, V, &EventContext<R::State>) -> Command<R::Event, R::Effect>,
{
    type State = R::State;
    type Event = R::Event;
    type Effect = R::Effect;

    fn reduce(
        &self,
        event: Self::Event,
        ctx: &EventContext<Self::State>,
    ) -> Command<Self::Event, Self::Effect> {
        let old_value = {
            // SAFETY: EventContext points to the active model for this dispatch.
            let model = unsafe { &*ctx.model_ptr() };
            (self.selector)(model)
        };

        let command = self.inner.reduce(event, ctx);

        let new_value = {
            // SAFETY: EventContext points to the active model for this dispatch.
            let model = unsafe { &*ctx.model_ptr() };
            (self.selector)(model)
        };

        if old_value == new_value {
            command
        } else {
            let reaction_command = (self.reaction)(old_value, new_value, ctx);
            command.and(reaction_command)
        }
    }
}

impl<R> Reducer for DebugReducer<R>
where
    R: Reducer,
    R::State: Clone + std::fmt::Debug,
    R::Event: std::fmt::Debug,
{
    type State = R::State;
    type Event = R::Event;
    type Effect = R::Effect;

    fn reduce(
        &self,
        event: Self::Event,
        ctx: &EventContext<Self::State>,
    ) -> Command<Self::Event, Self::Effect> {
        #[cfg(debug_assertions)]
        {
            let event_repr = format!("{event:?}");
            // SAFETY: EventContext points to the active model for this dispatch.
            let old_state = unsafe { (*ctx.model_ptr()).clone() };
            let command = self.inner.reduce(event, ctx);
            // SAFETY: The model pointer remains valid until dispatch returns.
            let new_state = unsafe { (*ctx.model_ptr()).clone() };
            self.print_changes(&event_repr, &old_state, &new_state);
            command
        }

        #[cfg(not(debug_assertions))]
        {
            self.inner.reduce(event, ctx)
        }
    }
}

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
        R: Reducer<State = S, Event = E, Effect = X> + 'static,
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
    use std::sync::{Arc, Mutex};

    use crate::command::CommandStep;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum CounterEvent {
        Increment(i32),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum CounterEffect {
        Changed(i32),
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct CounterState {
        value: i32,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum AppEvent {
        Ping,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum AppEffect {
        Audit(&'static str),
        Secondary(&'static str),
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct AppState;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum OnChangeEvent {
        SetValue(i32),
        BumpOther,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum OnChangeEffect {
        Base(&'static str),
        Watcher { old: i32, new: i32 },
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    struct OnChangeState {
        value: i32,
        other: i32,
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
    fn combine_runs_all_reducers() {
        let first = Reduce::new(
            |event: AppEvent, _ctx: &EventContext<AppState>| match event {
                AppEvent::Ping => Command::effect(AppEffect::Audit("first")),
            },
        );
        let second = Reduce::new(
            |event: AppEvent, _ctx: &EventContext<AppState>| match event {
                AppEvent::Ping => Command::effect(AppEffect::Secondary("second")),
            },
        );

        let reducer = combine(vec![first.boxed(), second.boxed()]);

        let mut state = AppState;
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
    fn on_change_fires_when_value_changes() {
        let reducer =
            Reduce::new(
                |event: OnChangeEvent, ctx: &EventContext<OnChangeState>| match event {
                    OnChangeEvent::SetValue(value) => {
                        // SAFETY: EventContext points to a live OnChangeState for this dispatch.
                        let state = unsafe { &mut *ctx.model_ptr() };
                        state.value = value;
                        Command::effect(OnChangeEffect::Base("set"))
                    }
                    OnChangeEvent::BumpOther => {
                        // SAFETY: EventContext points to a live OnChangeState for this dispatch.
                        let state = unsafe { &mut *ctx.model_ptr() };
                        state.other += 1;
                        Command::effect(OnChangeEffect::Base("other"))
                    }
                },
            )
            .on_change(
                |state: &OnChangeState| state.value,
                |old, new, _ctx| Command::effect(OnChangeEffect::Watcher { old, new }),
            );

        let mut state = OnChangeState::default();
        let ctx = EventContext::new(&mut state);
        let command = reducer.reduce(OnChangeEvent::SetValue(4), &ctx);

        assert_eq!(
            command.into_iter().collect::<Vec<_>>(),
            vec![
                CommandStep::Effect(OnChangeEffect::Base("set")),
                CommandStep::Effect(OnChangeEffect::Watcher { old: 0, new: 4 }),
            ]
        );
    }

    #[test]
    fn on_change_does_not_fire_when_value_unchanged() {
        let reducer =
            Reduce::new(
                |event: OnChangeEvent, ctx: &EventContext<OnChangeState>| match event {
                    OnChangeEvent::SetValue(value) => {
                        // SAFETY: EventContext points to a live OnChangeState for this dispatch.
                        let state = unsafe { &mut *ctx.model_ptr() };
                        state.value = value;
                        Command::effect(OnChangeEffect::Base("set"))
                    }
                    OnChangeEvent::BumpOther => {
                        // SAFETY: EventContext points to a live OnChangeState for this dispatch.
                        let state = unsafe { &mut *ctx.model_ptr() };
                        state.other += 1;
                        Command::effect(OnChangeEffect::Base("other"))
                    }
                },
            )
            .on_change(
                |state: &OnChangeState| state.value,
                |old, new, _ctx| Command::effect(OnChangeEffect::Watcher { old, new }),
            );

        let mut state = OnChangeState::default();
        let ctx = EventContext::new(&mut state);
        let command = reducer.reduce(OnChangeEvent::BumpOther, &ctx);

        assert_eq!(
            command.into_iter().collect::<Vec<_>>(),
            vec![CommandStep::Effect(OnChangeEffect::Base("other"))]
        );
    }

    #[test]
    fn on_change_receives_old_and_new_values() {
        let captures = Arc::new(Mutex::new(Vec::<(i32, i32)>::new()));
        let sink = Arc::clone(&captures);

        let reducer =
            Reduce::new(
                |event: OnChangeEvent, ctx: &EventContext<OnChangeState>| match event {
                    OnChangeEvent::SetValue(value) => {
                        // SAFETY: EventContext points to a live OnChangeState for this dispatch.
                        let state = unsafe { &mut *ctx.model_ptr() };
                        state.value = value;
                        Command::none()
                    }
                    OnChangeEvent::BumpOther => {
                        // SAFETY: EventContext points to a live OnChangeState for this dispatch.
                        let state = unsafe { &mut *ctx.model_ptr() };
                        state.other += 1;
                        Command::none()
                    }
                },
            )
            .on_change(
                |state: &OnChangeState| state.value,
                move |old, new, _ctx| {
                    let mut guard = sink.lock().expect("capture mutex poisoned");
                    guard.push((old, new));
                    Command::<OnChangeEvent, OnChangeEffect>::none()
                },
            );

        let mut state = OnChangeState::default();
        let ctx = EventContext::new(&mut state);

        let _first = reducer.reduce(OnChangeEvent::SetValue(5), &ctx);
        let _second = reducer.reduce(OnChangeEvent::SetValue(8), &ctx);
        let _third = reducer.reduce(OnChangeEvent::BumpOther, &ctx);

        let guard = captures.lock().expect("capture mutex poisoned");
        assert_eq!(guard.as_slice(), &[(0, 5), (5, 8)]);
    }

    #[test]
    fn debug_reducer_forwards_command_unchanged() {
        let plain_reducer =
            Reduce::new(
                |event: CounterEvent, ctx: &EventContext<CounterState>| match event {
                    CounterEvent::Increment(amount) => {
                        // SAFETY: EventContext points to a live CounterState for this dispatch.
                        let state = unsafe { &mut *ctx.model_ptr() };
                        state.value += amount;
                        Command::<CounterEvent, CounterEffect>::event(CounterEvent::Increment(
                            state.value,
                        ))
                        .and_effect(CounterEffect::Changed(state.value))
                    }
                },
            );

        let wrapped_reducer = Reduce::new(
            |event: CounterEvent, ctx: &EventContext<CounterState>| match event {
                CounterEvent::Increment(amount) => {
                    // SAFETY: EventContext points to a live CounterState for this dispatch.
                    let state = unsafe { &mut *ctx.model_ptr() };
                    state.value += amount;
                    Command::<CounterEvent, CounterEffect>::event(CounterEvent::Increment(
                        state.value,
                    ))
                    .and_effect(CounterEffect::Changed(state.value))
                }
            },
        )
        .debug_with(|_| {});

        let mut plain_state = CounterState::default();
        let plain_ctx = EventContext::new(&mut plain_state);
        let plain_command = plain_reducer.reduce(CounterEvent::Increment(3), &plain_ctx);

        let mut wrapped_state = CounterState::default();
        let wrapped_ctx = EventContext::new(&mut wrapped_state);
        let wrapped_command = wrapped_reducer.reduce(CounterEvent::Increment(3), &wrapped_ctx);

        assert_eq!(
            plain_command.into_iter().collect::<Vec<_>>(),
            wrapped_command.into_iter().collect::<Vec<_>>()
        );
    }

    #[test]
    fn debug_reducer_with_custom_printer_captures_output() {
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&logs);

        let reducer = Reduce::new(|event: CounterEvent, ctx: &EventContext<CounterState>| {
            match event {
                CounterEvent::Increment(amount) => {
                    // SAFETY: EventContext points to a live CounterState for this dispatch.
                    let state = unsafe { &mut *ctx.model_ptr() };
                    state.value += amount;
                    Command::<CounterEvent, CounterEffect>::none()
                }
            }
        })
        .debug_with(move |line| {
            let mut guard = sink.lock().expect("log mutex poisoned");
            guard.push(line.to_owned());
        });

        let mut state = CounterState::default();
        let ctx = EventContext::new(&mut state);
        let _command = reducer.reduce(CounterEvent::Increment(2), &ctx);

        let guard = logs.lock().expect("log mutex poisoned");
        assert_eq!(guard.len(), 1);
        assert!(guard[0].contains("received event: Increment(2)"));
        assert!(guard[0].contains("state changed"));
        assert!(guard[0].contains("old: CounterState { value: 0 }"));
        assert!(guard[0].contains("new: CounterState { value: 2 }"));
    }

    #[test]
    fn debug_reducer_reports_no_state_changes() {
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&logs);

        let reducer = Reduce::new(|event: CounterEvent, ctx: &EventContext<CounterState>| {
            match event {
                CounterEvent::Increment(amount) => {
                    // SAFETY: EventContext points to a live CounterState for this dispatch.
                    let state = unsafe { &mut *ctx.model_ptr() };
                    state.value += amount;
                    Command::<CounterEvent, CounterEffect>::none()
                }
            }
        })
        .debug_with(move |line| {
            let mut guard = sink.lock().expect("log mutex poisoned");
            guard.push(line.to_owned());
        });

        let mut state = CounterState::default();
        let ctx = EventContext::new(&mut state);
        let _command = reducer.reduce(CounterEvent::Increment(0), &ctx);

        let guard = logs.lock().expect("log mutex poisoned");
        assert_eq!(guard.len(), 1);
        assert!(guard[0].contains("received event: Increment(0)"));
        assert!(guard[0].contains("(no state changes)"));
    }
}
