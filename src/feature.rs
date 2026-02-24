use crate::command::Command;
use crate::executor::Task;
use crate::extract::{EffectContext, EventContext};
use crate::reducer::Reducer;

pub trait Feature: Send + Sync + 'static {
    type State: Send + 'static;
    type Event: Clone + Send + 'static;
    type Effect: Send + 'static;

    fn reduce(
        &self,
        event: Self::Event,
        ctx: &EventContext<Self::State>,
    ) -> Command<Self::Event, Self::Effect>;

    fn handle_effect(
        &self,
        effect: Self::Effect,
        ctx: &EffectContext,
    ) -> Task<Self::Event, Self::Effect> {
        let _ = (effect, ctx);
        Task::none()
    }
}

impl<F> Reducer for F
where
    F: Feature,
{
    type State = F::State;
    type Event = F::Event;
    type Effect = F::Effect;

    fn reduce(
        &self,
        event: Self::Event,
        ctx: &EventContext<Self::State>,
    ) -> Command<Self::Event, Self::Effect> {
        Feature::reduce(self, event, ctx)
    }
}
