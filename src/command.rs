use std::any::{Any, TypeId};
use std::hash::{Hash, Hasher};

use rustc_hash::FxHasher;
use smallvec::SmallVec;

/// Stable, type-tagged identifier for a cancellable effect slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CancelId {
    type_id: TypeId,
    hash: u64,
}

impl CancelId {
    #[must_use]
    pub fn new<T: Hash + 'static>(value: T) -> Self {
        if let Some(existing) = (&value as &dyn Any).downcast_ref::<Self>() {
            return *existing;
        }

        let mut hasher = FxHasher::default();
        value.hash(&mut hasher);

        Self {
            type_id: TypeId::of::<T>(),
            hash: hasher.finish(),
        }
    }

    #[must_use]
    fn map_namespace_with<Event, Effect, E2, X2, Namespace>(self, namespace: &Namespace) -> Self
    where
        Event: 'static,
        Effect: 'static,
        E2: 'static,
        X2: 'static,
        Namespace: Hash + 'static,
    {
        let mut hasher = FxHasher::default();
        self.type_id.hash(&mut hasher);
        self.hash.hash(&mut hasher);
        TypeId::of::<Event>().hash(&mut hasher);
        TypeId::of::<Effect>().hash(&mut hasher);
        TypeId::of::<E2>().hash(&mut hasher);
        TypeId::of::<X2>().hash(&mut hasher);
        TypeId::of::<Namespace>().hash(&mut hasher);
        namespace.hash(&mut hasher);

        Self {
            type_id: TypeId::of::<(Event, Effect, E2, X2, Namespace)>(),
            hash: hasher.finish(),
        }
    }
}

pub trait IntoCancelId {
    fn into_cancel_id(self) -> CancelId;
}

impl<T> IntoCancelId for T
where
    T: Hash + 'static,
{
    fn into_cancel_id(self) -> CancelId {
        CancelId::new(self)
    }
}

/// One atomic operation in the Core→Shell pipeline.
///
/// This is what actually happens when your event handler returns a Command.
/// Events go back to Core for immediate processing. Effects get queued for
/// async execution.
#[derive(Clone)]
pub enum CommandStep<Event, Effect> {
    Event(Event),
    Effect(Effect),
    Tracked { id: CancelId, effect: Effect },
    Cancel { id: CancelId },
}

impl<Event, Effect> PartialEq for CommandStep<Event, Effect>
where
    Event: PartialEq,
    Effect: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Event(a), Self::Event(b)) => a == b,
            (Self::Effect(a), Self::Effect(b)) => a == b,
            (
                Self::Tracked {
                    id: id_a,
                    effect: effect_a,
                },
                Self::Tracked {
                    id: id_b,
                    effect: effect_b,
                },
            ) => id_a == id_b && effect_a == effect_b,
            (Self::Cancel { id: id_a }, Self::Cancel { id: id_b }) => id_a == id_b,
            _ => false,
        }
    }
}

impl<Event, Effect> std::fmt::Debug for CommandStep<Event, Effect>
where
    Event: std::fmt::Debug,
    Effect: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Event(e) => f.debug_tuple("Event").field(e).finish(),
            Self::Effect(x) => f.debug_tuple("Effect").field(x).finish(),
            Self::Tracked { id, effect } => f
                .debug_struct("Tracked")
                .field("id", id)
                .field("effect", effect)
                .finish(),
            Self::Cancel { id } => f.debug_struct("Cancel").field("id", id).finish(),
        }
    }
}

/// The bridge between your pure event handler and the chaotic async world.
///
/// Commands are how you tell the Shell what to do without coupling your
/// event handler to implementation details. Think of it as a shopping list
/// for side effects - your event handler writes it, the Shell executes it.
///
/// Uses `SmallVec` because most commands contain 0-4 steps. If you're building
/// commands with more steps, reconsider your architecture - you're probably
/// doing too much in one event handler.
#[derive(Debug)]
pub struct Command<Event, Effect> {
    outputs: SmallVec<[CommandStep<Event, Effect>; 4]>,
}

impl<Event: Clone, Effect: Clone> Clone for Command<Event, Effect> {
    fn clone(&self) -> Self {
        Self {
            outputs: self.outputs.clone(),
        }
    }
}

impl<Event: Clone, Effect: Clone> Default for Command<Event, Effect> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Event: Clone, Effect: Clone> PartialEq for Command<Event, Effect>
where
    Event: PartialEq,
    Effect: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.outputs == other.outputs
    }
}

impl<Event, Effect> Command<Event, Effect> {
    /// Returns a command that does absolutely nothing.
    ///
    /// Use this when your event handler needs to update the model but doesn't
    /// need to trigger any side effects. It's the functional equivalent of
    /// telling the Shell "don't call us, we'll call you."
    ///
    /// # Example
    /// ```
    /// fn increment(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     model.counter += 1;
    ///     Command::none() // Model updated, no side effects needed
    /// }
    /// ```
    #[must_use]
    pub fn none() -> Self {
        Self {
            outputs: SmallVec::new(),
        }
    }

    /// Create a command with a single step.
    fn from_step(step: CommandStep<Event, Effect>) -> Self {
        let mut outputs = SmallVec::new();
        outputs.push(step);
        Self { outputs }
    }

    /// Creates a command that immediately triggers another event.
    ///
    /// The event gets processed synchronously in the same tick. No async
    /// boundary, no delay, and no chance for external interference between
    /// the chained events. Use this for breaking complex flows into smaller,
    /// testable pieces.
    pub fn event(event: impl Into<Event>) -> Self {
        Self::from_step(CommandStep::Event(event.into()))
    }

    /// Creates a command that triggers an async side effect.
    ///
    /// Effects run in the Shell's async context. They can spawn tasks, make
    /// HTTP requests, write files - all the dirty stuff your pure event handler
    /// shouldn't touch. Effects can send events back to Core when they're done.
    pub fn effect(effect: impl Into<Effect>) -> Self {
        Self::from_step(CommandStep::Effect(effect.into()))
    }

    /// Creates a command that schedules an effect in a cancellable slot.
    pub fn track(id: impl IntoCancelId, effect: impl Into<Effect>) -> Self {
        Self::from_step(CommandStep::Tracked {
            id: id.into_cancel_id(),
            effect: effect.into(),
        })
    }

    /// Creates a command that cancels the task currently running in `id`.
    pub fn cancel(id: impl IntoCancelId) -> Self {
        Self::from_step(CommandStep::Cancel {
            id: id.into_cancel_id(),
        })
    }

    /// Creates a command that fires multiple events in order.
    ///
    /// Events are processed sequentially in the order provided. Each event
    /// gets its own call to your event handler, so the model can change
    /// between events.
    pub fn events(events: impl IntoIterator<Item = Event>) -> Self {
        let outputs = events.into_iter().map(CommandStep::Event).collect();
        Self { outputs }
    }

    /// Combines multiple commands into one.
    ///
    /// Flattens all the steps from all commands into a single command.
    /// No magic, no deduplication, no ordering guarantees beyond what
    /// each individual command already provides. If you need specific
    /// ordering, build your commands carefully - this just concatenates.
    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        let mut outputs = SmallVec::new();
        for c in commands {
            outputs.extend(c.outputs);
        }
        Self { outputs }
    }

    /// Returns true if this command does nothing.
    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty()
    }

    /// Returns the total number of steps in this command.
    pub fn len(&self) -> usize {
        self.outputs.len()
    }

    #[must_use]
    pub(crate) fn has_cancellable_steps(&self) -> bool {
        self.outputs.iter().any(|step| {
            matches!(
                step,
                CommandStep::Tracked { .. } | CommandStep::Cancel { .. }
            )
        })
    }

    /// Transform event and effect types.
    ///
    /// Useful when embedding child commands into parent commands.
    ///
    /// This method is only for non-cancellable commands.
    ///
    /// If the command contains tracked/cancel steps, this method panics because
    /// implicit namespacing is ambiguous for repeated sibling instances.
    /// Use [`Command::map_namespaced`] with an explicit instance key.
    #[must_use]
    pub fn map<E2, X2, FE, FX>(self, fe: FE, fx: FX) -> Command<E2, X2>
    where
        FE: Fn(Event) -> E2,
        FX: Fn(Effect) -> X2,
        Event: 'static,
        Effect: 'static,
        E2: 'static,
        X2: 'static,
    {
        assert!(
            !self.has_cancellable_steps(),
            "Command::map cannot safely namespace tracked/cancel steps; use Command::map_namespaced(namespace, ...)"
        );

        let namespace = (std::any::type_name::<FE>(), std::any::type_name::<FX>());
        self.map_namespaced(namespace, fe, fx)
    }

    /// Transform event/effect types and namespace tracked cancellation slots
    /// with an explicit instance key.
    #[must_use]
    pub fn map_namespaced<Namespace, E2, X2>(
        self,
        namespace: Namespace,
        fe: impl Fn(Event) -> E2,
        fx: impl Fn(Effect) -> X2,
    ) -> Command<E2, X2>
    where
        Namespace: Hash + 'static,
        Event: 'static,
        Effect: 'static,
        E2: 'static,
        X2: 'static,
    {
        let outputs = self
            .outputs
            .into_iter()
            .map(|step| match step {
                CommandStep::Event(event) => CommandStep::Event(fe(event)),
                CommandStep::Effect(effect) => CommandStep::Effect(fx(effect)),
                CommandStep::Tracked { id, effect } => CommandStep::Tracked {
                    id: id.map_namespace_with::<Event, Effect, E2, X2, _>(&namespace),
                    effect: fx(effect),
                },
                CommandStep::Cancel { id } => CommandStep::Cancel {
                    id: id.map_namespace_with::<Event, Effect, E2, X2, _>(&namespace),
                },
            })
            .collect();

        Command { outputs }
    }

    /// Transform only the event type.
    #[must_use]
    pub fn map_event<E2>(self, f: impl Fn(Event) -> E2) -> Command<E2, Effect>
    where
        Event: 'static,
        Effect: 'static,
        E2: 'static,
    {
        self.map(f, std::convert::identity)
    }

    /// Transform only the effect type.
    #[must_use]
    pub fn map_effect<X2>(self, f: impl Fn(Effect) -> X2) -> Command<Event, X2>
    where
        Event: 'static,
        Effect: 'static,
        X2: 'static,
    {
        self.map(std::convert::identity, f)
    }

    // ---------------------------
    // Builder-style chaining API
    // ---------------------------

    /// Chain another event to this command.
    #[must_use]
    #[inline]
    pub fn and_event(mut self, event: impl Into<Event>) -> Self {
        self.outputs.push(CommandStep::Event(event.into()));
        self
    }

    /// Chain another effect to this command.
    #[must_use]
    #[inline]
    pub fn and_effect(mut self, effect: impl Into<Effect>) -> Self {
        self.outputs.push(CommandStep::Effect(effect.into()));
        self
    }

    /// Chain another tracked effect to this command.
    #[must_use]
    #[inline]
    pub fn and_track(mut self, id: impl IntoCancelId, effect: impl Into<Effect>) -> Self {
        self.outputs.push(CommandStep::Tracked {
            id: id.into_cancel_id(),
            effect: effect.into(),
        });
        self
    }

    /// Chain a cancellation step to this command.
    #[must_use]
    #[inline]
    pub fn and_cancel(mut self, id: impl IntoCancelId) -> Self {
        self.outputs.push(CommandStep::Cancel {
            id: id.into_cancel_id(),
        });
        self
    }

    /// Chain another command to this one (concatenates all steps).
    #[must_use]
    #[inline]
    pub fn and(mut self, other: Self) -> Self {
        self.outputs.extend(other.outputs);
        self
    }
}

impl<Event, Effect> IntoIterator for Command<Event, Effect> {
    type Item = CommandStep<Event, Effect>;
    type IntoIter = smallvec::IntoIter<[CommandStep<Event, Effect>; 4]>;
    fn into_iter(self) -> Self::IntoIter {
        self.outputs.into_iter()
    }
}

impl<'a, Event, Effect> IntoIterator for &'a Command<Event, Effect> {
    type Item = &'a CommandStep<Event, Effect>;
    type IntoIter = std::slice::Iter<'a, CommandStep<Event, Effect>>;
    fn into_iter(self) -> Self::IntoIter {
        self.outputs.iter()
    }
}

impl<Event: Clone, Effect: Clone> Command<Event, Effect> {
    /// Returns an iterator over the command steps.
    pub fn iter(&self) -> std::slice::Iter<'_, CommandStep<Event, Effect>> {
        self.outputs.iter()
    }
}

impl<Event, Effect> FromIterator<CommandStep<Event, Effect>> for Command<Event, Effect> {
    fn from_iter<I: IntoIterator<Item = CommandStep<Event, Effect>>>(iter: I) -> Self {
        Self {
            outputs: iter.into_iter().collect(),
        }
    }
}

impl<Event, Effect> From<()> for Command<Event, Effect> {
    fn from((): ()) -> Self {
        Self::none()
    }
}

/// ## Implementing `From<T>` for ergonomic conversions
///
/// Applications can add their own `From<T>` impls to convert domain types into
/// commands that target specific `Event`/`Effect` pairs:
///
/// ```rust
/// # use syzygy::command::{Command, CommandStep};
/// #[derive(Clone)] enum Event { KickOff }
/// #[derive(Clone)] enum Effect { Notify(String) }
///
/// impl From<Event> for Command<Event, Effect> {
///     fn from(event: Event) -> Self {
///         Command::event(event)
///     }
/// }
///
/// impl From<Effect> for Command<Event, Effect> {
///     fn from(effect: Effect) -> Self {
///         Command::effect(effect)
///     }
/// }
/// ```
///
/// If `Event` and `Effect` are the same type you cannot implement both traits
/// due to Rust's coherence rules. In that case call `Command::event` and
/// `Command::effect` directly at the call site.
/// Free-function helpers for building [`Command`] values.
///
/// These functions mirror the inherent constructors on [`Command`] but live in a module that can
/// be glob-imported from the prelude (`use syzygy::prelude::command::*;`) for quick prototyping.
pub mod builders {
    use super::{Command, IntoCancelId};

    /// Construct a no-op command.
    #[inline]
    #[must_use]
    pub fn none<Event, Effect>() -> Command<Event, Effect> {
        Command::none()
    }

    /// Emit an immediate event back into Core.
    #[inline]
    #[must_use]
    pub fn event<Event, Effect>(event: impl Into<Event>) -> Command<Event, Effect> {
        Command::event(event)
    }

    /// Emit a sequence of events.
    #[inline]
    #[must_use]
    pub fn events<Event, Effect>(
        events: impl IntoIterator<Item = Event>,
    ) -> Command<Event, Effect> {
        Command::events(events)
    }

    /// Schedule a single effect.
    #[inline]
    #[must_use]
    pub fn effect<Event, Effect>(effect: impl Into<Effect>) -> Command<Event, Effect> {
        Command::effect(effect)
    }

    /// Schedule a tracked effect slot.
    #[inline]
    #[must_use]
    pub fn track<Event, Effect>(
        id: impl IntoCancelId,
        effect: impl Into<Effect>,
    ) -> Command<Event, Effect> {
        Command::track(id, effect)
    }

    /// Cancel a tracked effect slot.
    #[inline]
    #[must_use]
    pub fn cancel<Event, Effect>(id: impl IntoCancelId) -> Command<Event, Effect> {
        Command::cancel(id)
    }

    /// Flatten multiple commands into one.
    #[inline]
    #[must_use]
    pub fn batch<Event, Effect>(
        commands: impl IntoIterator<Item = Command<Event, Effect>>,
    ) -> Command<Event, Effect> {
        Command::batch(commands)
    }
}

#[cfg(test)]
mod tests {
    use super::{CancelId, Command, CommandStep};

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ChildEvent {
        Ready,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ParentEvent {
        Child(ChildEvent),
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ChildEffect {
        Load,
        Save,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum ParentEffect {
        Child(ChildEffect),
    }

    #[test]
    fn map_event_wraps_events() {
        let mapped: Command<ParentEvent, ChildEffect> =
            Command::event(ChildEvent::Ready).map_event(ParentEvent::Child);

        assert_eq!(
            mapped.into_iter().collect::<Vec<_>>(),
            vec![CommandStep::Event(ParentEvent::Child(ChildEvent::Ready))]
        );
    }

    #[test]
    fn map_effect_wraps_effects() {
        let mapped: Command<ChildEvent, ParentEffect> =
            Command::effect(ChildEffect::Load).map_effect(ParentEffect::Child);

        assert_eq!(
            mapped.into_iter().collect::<Vec<_>>(),
            vec![CommandStep::Effect(ParentEffect::Child(ChildEffect::Load))]
        );
    }

    #[test]
    fn map_transforms_events_and_effects() {
        let mapped: Command<ParentEvent, ParentEffect> = Command::event(ChildEvent::Ready)
            .and_effect(ChildEffect::Load)
            .map(ParentEvent::Child, ParentEffect::Child);

        assert_eq!(
            mapped.into_iter().collect::<Vec<_>>(),
            vec![
                CommandStep::Event(ParentEvent::Child(ChildEvent::Ready)),
                CommandStep::Effect(ParentEffect::Child(ChildEffect::Load)),
            ]
        );
    }

    #[test]
    fn map_effect_transforms_multiple_effect_steps() {
        let mapped: Command<ChildEvent, ParentEffect> = Command::effect(ChildEffect::Load)
            .and_effect(ChildEffect::Save)
            .map_effect(ParentEffect::Child);

        assert_eq!(
            mapped.into_iter().collect::<Vec<_>>(),
            vec![
                CommandStep::Effect(ParentEffect::Child(ChildEffect::Load)),
                CommandStep::Effect(ParentEffect::Child(ChildEffect::Save)),
            ]
        );
    }

    #[test]
    fn map_on_none_stays_none() {
        let mapped: Command<ParentEvent, ParentEffect> =
            Command::<ChildEvent, ChildEffect>::none().map(ParentEvent::Child, ParentEffect::Child);

        assert!(mapped.is_empty());
        assert_eq!(mapped.into_iter().count(), 0);
    }

    #[test]
    fn tracked_steps_map_effects_are_namespaced_consistently() {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        enum Slot {
            Active,
        }

        let mapped: Command<ChildEvent, ParentEffect> =
            Command::track(Slot::Active, ChildEffect::Load)
                .and_cancel(Slot::Active)
                .map_namespaced(0_u8, std::convert::identity, ParentEffect::Child);

        let steps = mapped.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 2);

        match (&steps[0], &steps[1]) {
            (
                CommandStep::Tracked {
                    id: tracked_id,
                    effect,
                },
                CommandStep::Cancel { id: cancel_id },
            ) => {
                assert_eq!(*effect, ParentEffect::Child(ChildEffect::Load));
                assert_eq!(tracked_id, cancel_id);
                assert_ne!(*tracked_id, CancelId::new(Slot::Active));
            }
            _ => panic!("expected tracked + cancel steps"),
        }
    }

    #[test]
    fn map_namespaces_tracked_slots_across_component_boundaries() {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        enum Slot {
            Active,
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        struct ChildEventA;

        #[derive(Debug, Clone, PartialEq, Eq)]
        struct ChildEventB;

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ChildEffectA {
            Work,
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ChildEffectB {
            Work,
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ParentEvent {
            A(ChildEventA),
            B(ChildEventB),
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ParentEffect {
            A(ChildEffectA),
            B(ChildEffectB),
        }

        let a = Command::<ChildEventA, ChildEffectA>::track(Slot::Active, ChildEffectA::Work)
            .map_namespaced((), ParentEvent::A, ParentEffect::A)
            .into_iter()
            .collect::<Vec<_>>();
        let b = Command::<ChildEventB, ChildEffectB>::track(Slot::Active, ChildEffectB::Work)
            .map_namespaced((), ParentEvent::B, ParentEffect::B)
            .into_iter()
            .collect::<Vec<_>>();

        let a_id = match &a[0] {
            CommandStep::Tracked { id, .. } => *id,
            _ => panic!("expected tracked step"),
        };
        let b_id = match &b[0] {
            CommandStep::Tracked { id, .. } => *id,
            _ => panic!("expected tracked step"),
        };

        assert_ne!(a_id, b_id);
    }

    #[test]
    fn map_panics_for_cancellable_steps_without_explicit_namespace() {
        #[derive(Debug, Clone, PartialEq, Eq)]
        struct ChildEvent;

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ChildEffect {
            Work,
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ParentEvent {
            Child(ChildEvent),
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ParentEffect {
            Child(ChildEffect),
        }

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = Command::<ChildEvent, ChildEffect>::track(1_u8, ChildEffect::Work)
                .map(ParentEvent::Child, ParentEffect::Child);
        }));

        assert!(result.is_err());
    }

    #[test]
    fn map_namespaced_disambiguates_identical_child_types() {
        #[derive(Debug, Clone, PartialEq, Eq)]
        struct ChildEvent;

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ChildEffect {
            Work,
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ParentEvent {
            Child(ChildEvent),
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        enum ParentEffect {
            Child(ChildEffect),
        }

        let a = Command::<ChildEvent, ChildEffect>::track(1_u8, ChildEffect::Work)
            .map_namespaced(0_u8, ParentEvent::Child, ParentEffect::Child)
            .into_iter()
            .collect::<Vec<_>>();
        let b = Command::<ChildEvent, ChildEffect>::track(1_u8, ChildEffect::Work)
            .map_namespaced(1_u8, ParentEvent::Child, ParentEffect::Child)
            .into_iter()
            .collect::<Vec<_>>();

        let a_id = match &a[0] {
            CommandStep::Tracked { id, .. } => *id,
            _ => panic!("expected tracked step"),
        };
        let b_id = match &b[0] {
            CommandStep::Tracked { id, .. } => *id,
            _ => panic!("expected tracked step"),
        };

        assert_ne!(a_id, b_id);
    }

    #[test]
    fn cancel_id_preserves_type_identity() {
        let u32_id = CancelId::new(7_u32);
        let u64_id = CancelId::new(7_u64);

        assert_ne!(u32_id, u64_id);
    }
}
