use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Weak};

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

static NEXT_TASK_LEASE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
struct TaskLeaseOwner;

#[derive(Debug, Clone)]
enum TaskLeaseOwnerRef {
    Strong(Arc<TaskLeaseOwner>),
    Weak(Weak<TaskLeaseOwner>),
}

impl TaskLeaseOwnerRef {
    fn downgrade(&self) -> Weak<TaskLeaseOwner> {
        match self {
            Self::Strong(owner) => Arc::downgrade(owner),
            Self::Weak(owner) => Weak::clone(owner),
        }
    }
}

#[derive(Debug)]
struct TaskLeaseState {
    id: u64,
    owner: TaskLeaseOwnerRef,
}

/// Ownership token for abortable work.
///
/// As long as at least one clone of a `TaskLease` exists, the associated
/// abortable task is allowed to keep running. Once the last owner disappears,
/// the shell cancels the task on the next `drain`/`step` cycle.
#[derive(Clone)]
pub struct TaskLease {
    inner: Arc<TaskLeaseState>,
}

#[derive(Clone, Debug, Default)]
pub struct TaskLeaseScope {
    leases: Rc<RefCell<FxHashMap<u64, TaskLease>>>,
}

#[derive(Clone, Debug)]
pub(crate) struct TaskLeaseWeak {
    id: u64,
    owner: Weak<TaskLeaseOwner>,
}

impl TaskLeaseWeak {
    #[must_use]
    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    #[must_use]
    pub(crate) fn has_owner(&self) -> bool {
        self.owner.strong_count() > 0
    }
}

impl TaskLeaseScope {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    fn remap(&self, lease: &TaskLease) -> TaskLease {
        if let Some(mapped) = self.leases.borrow().get(&lease.id()).cloned() {
            return mapped;
        }

        let mapped = TaskLease::borrowed(lease.owner_weak());
        self.leases.borrow_mut().insert(lease.id(), mapped.clone());
        mapped
    }
}

impl TaskLease {
    #[must_use]
    pub fn new() -> Self {
        let owner = Arc::new(TaskLeaseOwner);
        let id = NEXT_TASK_LEASE_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: Arc::new(TaskLeaseState {
                id,
                owner: TaskLeaseOwnerRef::Strong(owner),
            }),
        }
    }

    #[must_use]
    pub fn cancel_command<Event, Effect>(&self) -> Command<Event, Effect> {
        Command::cancel(self)
    }

    #[must_use]
    pub fn process_write_command<Event, Effect>(
        &self,
        bytes: impl Into<Vec<u8>>,
    ) -> Command<Event, Effect> {
        Command::process_write(self, bytes)
    }

    #[must_use]
    pub fn process_close_stdin_command<Event, Effect>(&self) -> Command<Event, Effect> {
        Command::process_close_stdin(self)
    }

    #[must_use]
    pub(crate) fn id(&self) -> u64 {
        self.inner.id
    }

    #[must_use]
    pub(crate) fn downgrade(&self) -> TaskLeaseWeak {
        TaskLeaseWeak {
            id: self.id(),
            owner: self.owner_weak(),
        }
    }

    #[must_use]
    fn owner_weak(&self) -> Weak<TaskLeaseOwner> {
        self.inner.owner.downgrade()
    }

    #[must_use]
    fn borrowed(owner: Weak<TaskLeaseOwner>) -> Self {
        let id = NEXT_TASK_LEASE_ID.fetch_add(1, Ordering::Relaxed);
        Self {
            inner: Arc::new(TaskLeaseState {
                id,
                owner: TaskLeaseOwnerRef::Weak(owner),
            }),
        }
    }
}

impl Default for TaskLease {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for TaskLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskLease").field("id", &self.id()).finish()
    }
}

impl PartialEq for TaskLease {
    fn eq(&self, other: &Self) -> bool {
        self.id() == other.id()
    }
}

impl Eq for TaskLease {}

impl Hash for TaskLease {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

impl From<&TaskLease> for TaskLease {
    fn from(value: &TaskLease) -> Self {
        value.clone()
    }
}

/// Explicit model state helper for a single abortable task owner.
///
/// `AbortSlot` keeps the current lease in model state and provides ergonomic
/// helpers for the common "start / replace / cancel / clear" flow without
/// hiding ownership. The slot itself does not execute anything; it only stores
/// the lease and builds [`Command`] values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbortSlot {
    lease: Option<TaskLease>,
}

impl AbortSlot {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start or replace the abortable effect owned by this slot.
    ///
    /// If the slot already owns a lease, the returned command cancels the old
    /// task explicitly before starting the new one.
    #[must_use]
    pub fn start<Event, Effect>(&mut self, effect: impl Into<Effect>) -> Command<Event, Effect> {
        let next = TaskLease::new();
        let previous = self.lease.replace(next.clone());
        let command = Command::abortable(next, effect);

        match previous {
            Some(previous) => Command::cancel(previous).and(command),
            None => command,
        }
    }

    /// Cancel the currently owned abortable task, if any.
    #[must_use]
    pub fn cancel<Event, Effect>(&mut self) -> Command<Event, Effect> {
        match self.take() {
            Some(lease) => Command::cancel(lease),
            None => Command::none(),
        }
    }

    /// Drop the current owner without emitting a command.
    ///
    /// The shell will observe owner loss and cancel the task on the next
    /// `step`/`drain` cycle.
    pub fn clear(&mut self) {
        self.lease = None;
    }

    /// Write bytes to the currently owned interactive process, if any.
    #[must_use]
    pub fn write<Event, Effect>(&self, bytes: impl Into<Vec<u8>>) -> Command<Event, Effect> {
        match self.lease() {
            Some(lease) => Command::process_write(lease, bytes),
            None => Command::none(),
        }
    }

    /// Close stdin for the currently owned interactive process, if any.
    #[must_use]
    pub fn close_stdin<Event, Effect>(&self) -> Command<Event, Effect> {
        match self.lease() {
            Some(lease) => Command::process_close_stdin(lease),
            None => Command::none(),
        }
    }

    /// Remove and return the current lease.
    pub fn take(&mut self) -> Option<TaskLease> {
        self.lease.take()
    }

    #[must_use]
    pub fn lease(&self) -> Option<&TaskLease> {
        self.lease.as_ref()
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.lease.is_some()
    }
}

/// One atomic operation in Syzygy's runtime command vocabulary.
///
/// This is what actually happens when your event handler returns a `Command`.
/// It is intentionally runtime-facing (Core + Shell orchestration), not a
/// purely abstract event/effect container.
///
/// - `Event` and `Effect` steps encode boundary-safe app intent.
/// - `Abortable` / `Cancel` steps encode lease-owned shell cancellation.
/// - `ProcessWrite` / `ProcessCloseStdin` steps encode explicit shell-owned
///   interactive process capabilities (no raw process handles in model state).
#[derive(Clone)]
pub enum CommandStep<Event, Effect> {
    Event(Event),
    Effect(Effect),
    Abortable { lease: TaskLease, effect: Effect },
    Cancel { lease: TaskLease },
    ProcessWrite { lease: TaskLease, bytes: Vec<u8> },
    ProcessCloseStdin { lease: TaskLease },
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
                Self::Abortable {
                    lease: lease_a,
                    effect: effect_a,
                },
                Self::Abortable {
                    lease: lease_b,
                    effect: effect_b,
                },
            ) => lease_a == lease_b && effect_a == effect_b,
            (Self::Cancel { lease: lease_a }, Self::Cancel { lease: lease_b }) => {
                lease_a == lease_b
            }
            (
                Self::ProcessWrite {
                    lease: lease_a,
                    bytes: bytes_a,
                },
                Self::ProcessWrite {
                    lease: lease_b,
                    bytes: bytes_b,
                },
            ) => lease_a == lease_b && bytes_a == bytes_b,
            (
                Self::ProcessCloseStdin { lease: lease_a },
                Self::ProcessCloseStdin { lease: lease_b },
            ) => lease_a == lease_b,
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
            Self::Abortable { lease, effect } => f
                .debug_struct("Abortable")
                .field("lease", lease)
                .field("effect", effect)
                .finish(),
            Self::Cancel { lease } => f.debug_struct("Cancel").field("lease", lease).finish(),
            Self::ProcessWrite { lease, bytes } => f
                .debug_struct("ProcessWrite")
                .field("lease", lease)
                .field("bytes_len", &bytes.len())
                .finish(),
            Self::ProcessCloseStdin { lease } => f
                .debug_struct("ProcessCloseStdin")
                .field("lease", lease)
                .finish(),
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

    /// Creates a command step that emits another event back into Core.
    ///
    /// Runtime processing is step-based: emitted events are queued and handled
    /// on the next Core drain cycle, not inline in the current handler call.
    /// Use this for explicit follow-up transitions while keeping event flow
    /// deterministic and observable.
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

    /// Creates a command that schedules an abortable effect under `lease`.
    pub fn abortable(lease: impl Into<TaskLease>, effect: impl Into<Effect>) -> Self {
        Self::from_step(CommandStep::Abortable {
            lease: lease.into(),
            effect: effect.into(),
        })
    }

    /// Creates a command that cancels the abortable task owned by `lease`.
    pub fn cancel(lease: impl Into<TaskLease>) -> Self {
        Self::from_step(CommandStep::Cancel {
            lease: lease.into(),
        })
    }

    /// Creates a command that writes bytes to a shell-owned interactive process.
    ///
    /// The process is addressed by `TaskLease`; this does not expose raw child
    /// handles to application code.
    pub fn process_write(lease: impl Into<TaskLease>, bytes: impl Into<Vec<u8>>) -> Self {
        Self::from_step(CommandStep::ProcessWrite {
            lease: lease.into(),
            bytes: bytes.into(),
        })
    }

    /// Creates a command that closes stdin for a shell-owned interactive process.
    ///
    /// Like [`Self::process_write`], this is lease-addressed capability control.
    pub fn process_close_stdin(lease: impl Into<TaskLease>) -> Self {
        Self::from_step(CommandStep::ProcessCloseStdin {
            lease: lease.into(),
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
    pub(crate) fn has_lease_steps(&self) -> bool {
        self.outputs.iter().any(|step| {
            matches!(
                step,
                CommandStep::Abortable { .. }
                    | CommandStep::Cancel { .. }
                    | CommandStep::ProcessWrite { .. }
                    | CommandStep::ProcessCloseStdin { .. }
            )
        })
    }

    /// Transform event and effect types.
    ///
    /// Useful when embedding child commands into parent commands.
    #[must_use]
    pub fn map<E2, X2, FE, FX>(self, fe: FE, fx: FX) -> Command<E2, X2>
    where
        FE: Fn(Event) -> E2,
        FX: Fn(Effect) -> X2,
    {
        assert!(
            !self.has_lease_steps(),
            "Command::map cannot safely remap lease-addressed steps; use Command::map_scoped(scope, ...)"
        );

        self.map_scoped(&TaskLeaseScope::new(), fe, fx)
    }

    /// Transform event/effect types and explicitly remap abortable task leases.
    #[must_use]
    pub fn map_scoped<E2, X2, FE, FX>(
        self,
        scope: &TaskLeaseScope,
        fe: FE,
        fx: FX,
    ) -> Command<E2, X2>
    where
        FE: Fn(Event) -> E2,
        FX: Fn(Effect) -> X2,
    {
        let outputs = self
            .outputs
            .into_iter()
            .map(|step| match step {
                CommandStep::Event(event) => CommandStep::Event(fe(event)),
                CommandStep::Effect(effect) => CommandStep::Effect(fx(effect)),
                CommandStep::Abortable { lease, effect } => CommandStep::Abortable {
                    lease: scope.remap(&lease),
                    effect: fx(effect),
                },
                CommandStep::Cancel { lease } => CommandStep::Cancel {
                    lease: scope.remap(&lease),
                },
                CommandStep::ProcessWrite { lease, bytes } => CommandStep::ProcessWrite {
                    lease: scope.remap(&lease),
                    bytes,
                },
                CommandStep::ProcessCloseStdin { lease } => CommandStep::ProcessCloseStdin {
                    lease: scope.remap(&lease),
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

    /// Chain another abortable effect to this command.
    #[must_use]
    #[inline]
    pub fn and_abortable(mut self, lease: impl Into<TaskLease>, effect: impl Into<Effect>) -> Self {
        self.outputs.push(CommandStep::Abortable {
            lease: lease.into(),
            effect: effect.into(),
        });
        self
    }

    /// Chain a cancellation step to this command.
    #[must_use]
    #[inline]
    pub fn and_cancel(mut self, lease: impl Into<TaskLease>) -> Self {
        self.outputs.push(CommandStep::Cancel {
            lease: lease.into(),
        });
        self
    }

    /// Chain a process stdin write step to this command.
    #[must_use]
    #[inline]
    pub fn and_process_write(
        mut self,
        lease: impl Into<TaskLease>,
        bytes: impl Into<Vec<u8>>,
    ) -> Self {
        self.outputs.push(CommandStep::ProcessWrite {
            lease: lease.into(),
            bytes: bytes.into(),
        });
        self
    }

    /// Chain a process stdin close step to this command.
    #[must_use]
    #[inline]
    pub fn and_process_close_stdin(mut self, lease: impl Into<TaskLease>) -> Self {
        self.outputs.push(CommandStep::ProcessCloseStdin {
            lease: lease.into(),
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
    use super::{Command, TaskLease};

    /// Construct a no-op command.
    #[inline]
    #[must_use]
    pub fn none<Event, Effect>() -> Command<Event, Effect> {
        Command::none()
    }

    /// Emit an event back into Core for the next drain cycle.
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

    /// Schedule an abortable effect under a task lease.
    #[inline]
    #[must_use]
    pub fn abortable<Event, Effect>(
        lease: impl Into<TaskLease>,
        effect: impl Into<Effect>,
    ) -> Command<Event, Effect> {
        Command::abortable(lease, effect)
    }

    /// Cancel an abortable effect lease.
    #[inline]
    #[must_use]
    pub fn cancel<Event, Effect>(lease: impl Into<TaskLease>) -> Command<Event, Effect> {
        Command::cancel(lease)
    }

    /// Write bytes to a shell-owned interactive process addressed by a lease.
    #[inline]
    #[must_use]
    pub fn process_write<Event, Effect>(
        lease: impl Into<TaskLease>,
        bytes: impl Into<Vec<u8>>,
    ) -> Command<Event, Effect> {
        Command::process_write(lease, bytes)
    }

    /// Close stdin for a shell-owned interactive process addressed by a lease.
    #[inline]
    #[must_use]
    pub fn process_close_stdin<Event, Effect>(
        lease: impl Into<TaskLease>,
    ) -> Command<Event, Effect> {
        Command::process_close_stdin(lease)
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
    use super::{AbortSlot, Command, CommandStep, TaskLease, TaskLeaseScope};
    use crate::test_store::assert_panic_contains;

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
    fn map_rejects_abortable_steps_without_explicit_scope() {
        let lease = TaskLease::new();

        assert_panic_contains(
            "Command::map cannot safely remap lease-addressed steps",
            || {
                let _mapped: Command<ChildEvent, ParentEffect> =
                    Command::abortable(&lease, ChildEffect::Load)
                        .and_cancel(&lease)
                        .map(std::convert::identity, ParentEffect::Child);
            },
        );
    }

    #[test]
    fn scoped_mapping_keeps_abortable_and_cancel_leases_aligned() {
        let lease = TaskLease::new();
        let scope = TaskLeaseScope::new();
        let mapped: Command<ChildEvent, ParentEffect> =
            Command::abortable(&lease, ChildEffect::Load)
                .and_cancel(&lease)
                .map_scoped(&scope, std::convert::identity, ParentEffect::Child);

        let steps = mapped.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 2);

        match (&steps[0], &steps[1]) {
            (
                CommandStep::Abortable {
                    lease: abortable_lease,
                    effect,
                },
                CommandStep::Cancel {
                    lease: cancelled_lease,
                },
            ) => {
                assert_eq!(*effect, ParentEffect::Child(ChildEffect::Load));
                assert_eq!(abortable_lease, cancelled_lease);
                assert_ne!(abortable_lease, &lease);
            }
            _ => panic!("expected abortable + cancel steps"),
        }
    }

    #[test]
    fn distinct_scopes_disambiguate_reused_abortable_commands() {
        let lease = TaskLease::new();
        let child = Command::<ChildEvent, ChildEffect>::abortable(&lease, ChildEffect::Load);

        let first_scope = TaskLeaseScope::new();
        let second_scope = TaskLeaseScope::new();
        let first_command: Command<ParentEvent, ParentEffect> =
            child
                .clone()
                .map_scoped(&first_scope, ParentEvent::Child, ParentEffect::Child);
        let second_command: Command<ParentEvent, ParentEffect> =
            child.map_scoped(&second_scope, ParentEvent::Child, ParentEffect::Child);

        let Some(CommandStep::Abortable {
            lease: first_lease, ..
        }) = first_command.into_iter().next()
        else {
            panic!("expected abortable step");
        };
        let Some(CommandStep::Abortable {
            lease: second_lease,
            ..
        }) = second_command.into_iter().next()
        else {
            panic!("expected abortable step");
        };

        assert_ne!(first_lease, second_lease);
        assert_ne!(first_lease, lease);
        assert_ne!(second_lease, lease);
    }

    #[test]
    fn task_lease_new_is_unique() {
        let first = TaskLease::new();
        let second = TaskLease::new();

        assert_ne!(first, second);
    }

    #[test]
    fn abort_slot_start_stores_lease_and_emits_abortable_step() {
        let mut slot = AbortSlot::new();
        let command: Command<ChildEvent, ChildEffect> = slot.start(ChildEffect::Load);

        let current = slot.lease().cloned().expect("slot should store a lease");
        let steps = command.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);

        match &steps[0] {
            CommandStep::Abortable { lease, effect } => {
                assert_eq!(lease, &current);
                assert_eq!(*effect, ChildEffect::Load);
            }
            _ => panic!("expected abortable step"),
        }
    }

    #[test]
    fn abort_slot_restart_cancels_previous_lease_before_starting_next() {
        let mut slot = AbortSlot::new();
        let _: Command<ChildEvent, ChildEffect> = slot.start(ChildEffect::Load);
        let previous = slot.lease().cloned().expect("slot should store a lease");

        let command: Command<ChildEvent, ChildEffect> = slot.start(ChildEffect::Save);
        let current = slot.lease().cloned().expect("slot should store a lease");

        assert_ne!(previous, current);

        let steps = command.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 2);

        match (&steps[0], &steps[1]) {
            (
                CommandStep::Cancel {
                    lease: cancelled_lease,
                },
                CommandStep::Abortable { lease, effect },
            ) => {
                assert_eq!(cancelled_lease, &previous);
                assert_eq!(lease, &current);
                assert_eq!(*effect, ChildEffect::Save);
            }
            _ => panic!("expected cancel + abortable steps"),
        }
    }

    #[test]
    fn abort_slot_cancel_clears_lease_and_emits_cancel_step() {
        let mut slot = AbortSlot::new();
        let _: Command<ChildEvent, ChildEffect> = slot.start(ChildEffect::Load);
        let current = slot.lease().cloned().expect("slot should store a lease");

        let command: Command<ChildEvent, ChildEffect> = slot.cancel();

        assert!(!slot.is_active());

        let steps = command.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 1);
        match &steps[0] {
            CommandStep::Cancel { lease } => assert_eq!(lease, &current),
            _ => panic!("expected cancel step"),
        }
    }

    #[test]
    fn abort_slot_write_without_active_lease_is_noop() {
        let slot = AbortSlot::new();
        let command: Command<ChildEvent, ChildEffect> = slot.write(b"hello".to_vec());

        assert!(command.is_empty());
    }

    #[test]
    fn abort_slot_close_stdin_without_active_lease_is_noop() {
        let slot = AbortSlot::new();
        let command: Command<ChildEvent, ChildEffect> = slot.close_stdin();

        assert!(command.is_empty());
    }

    #[test]
    fn scoped_mapping_keeps_process_control_leases_aligned() {
        let lease = TaskLease::new();
        let scope = TaskLeaseScope::new();
        let mapped: Command<ChildEvent, ParentEffect> = Command::process_write(&lease, b"abc")
            .and_process_close_stdin(&lease)
            .map_scoped(&scope, std::convert::identity, ParentEffect::Child);

        let steps = mapped.into_iter().collect::<Vec<_>>();
        assert_eq!(steps.len(), 2);

        match (&steps[0], &steps[1]) {
            (
                CommandStep::ProcessWrite {
                    lease: write_lease,
                    bytes,
                },
                CommandStep::ProcessCloseStdin { lease: close_lease },
            ) => {
                assert_eq!(bytes, b"abc");
                assert_eq!(write_lease, close_lease);
                assert_ne!(write_lease, &lease);
            }
            _ => panic!("expected write + close steps"),
        }
    }
}
