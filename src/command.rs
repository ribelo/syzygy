// trimmed public testing helpers; no external Sender used here anymore
use smallvec::SmallVec;
use std::time::Duration;

pub mod executor;
pub mod effects_builder;

// Re-export for convenience - this is the main API users should use
pub use effects_builder::Effects;

/// A step in a Command - events, effects, or coordination patterns
pub enum CommandStep<Event, Effect> {
    /// Single event to be processed by Core
    Event(Event),
    /// Single effect to be executed by Shell
    Effect(Effect),
    /// Batch execution - effects run sequentially (same as single effects, but batched for efficiency)
    Batch(Vec<Effect>),
    /// Unified group execution with policy and optional barrier (legacy internal for futures paths)
    Group {
        effects: Vec<Effect>,
        mode: GroupMode,
        barrier: Option<Event>,
        timeout_per: Option<Duration>,
    },
    /// Merge: stream merge; forwards items from all streams as they arrive; optional barrier when all complete
    Merge { effects: Vec<Effect>, barrier_event: Option<Event> },
    /// Join: futures join; forwards each Single result; optional per-effect timeout; optional barrier when all complete
    Join { effects: Vec<Effect>, timeout_per: Option<Duration>, barrier_event: Option<Event> },
    /// Race: first to produce event wins; optional timeout; optional barrier when winner selected
    Race { effects: Vec<Effect>, timeout_per: Option<Duration>, barrier_event: Option<Event> },
    /// Chain: sequential stream processing with optional barrier when final completes
    Chain { effects: Vec<Effect>, barrier_event: Option<Event> },
}

/// Group execution policy
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupMode {
    Parallel,
    Race,
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
            (Self::Batch(a), Self::Batch(b)) => a == b,
            (
                Self::Group { effects: ae, mode: am, barrier: ab, timeout_per: at },
                Self::Group { effects: be, mode: bm, barrier: bb, timeout_per: bt },
            ) => ae == be && am == bm && ab == bb && at == bt,
            (
                Self::Merge { effects: ae, barrier_event: ab },
                Self::Merge { effects: be, barrier_event: bb },
            ) => ae == be && ab == bb,
            (
                Self::Join { effects: ae, timeout_per: at, barrier_event: ab },
                Self::Join { effects: be, timeout_per: bt, barrier_event: bb },
            ) => ae == be && at == bt && ab == bb,
            (
                Self::Race { effects: ae, timeout_per: at, barrier_event: ab },
                Self::Race { effects: be, timeout_per: bt, barrier_event: bb },
            ) => ae == be && at == bt && ab == bb,
            (
                Self::Chain { effects: ae, barrier_event: ab },
                Self::Chain { effects: be, barrier_event: bb },
            ) => ae == be && ab == bb,
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
            Self::Batch(v) => f.debug_tuple("Batch").field(v).finish(),
            Self::Group { effects, mode, barrier, timeout_per } => {
                f.debug_struct("Group")
                    .field("effects", effects)
                    .field("mode", mode)
                    .field("barrier", barrier)
                    .field("timeout_per", timeout_per)
                    .finish()
            }
            Self::Merge { effects, barrier_event } => {
                f.debug_struct("Merge").field("effects", effects).field("barrier_event", barrier_event).finish()
            }
            Self::Join { effects, timeout_per, barrier_event } => {
                f.debug_struct("Join").field("effects", effects).field("timeout_per", timeout_per).field("barrier_event", barrier_event).finish()
            }
            Self::Race { effects, timeout_per, barrier_event } => {
                f.debug_struct("Race").field("effects", effects).field("timeout_per", timeout_per).field("barrier_event", barrier_event).finish()
            }
            Self::Chain { effects, barrier_event } => {
                f.debug_struct("Chain").field("effects", effects).field("barrier_event", barrier_event).finish()
            }
        }
    }
}

// CommandContext removed from public API; prefer Shell + Runner for integration

/// Command orchestrates effects and events as pure data.
///
/// Commands are simple, testable data structures that describe what should happen,
/// not how it should happen. The Shell interprets Commands and executes the
/// described effects asynchronously.
///
/// Most commands (1-4 outputs) are stored inline with zero heap allocation.
/// Larger commands automatically spill to the heap.
#[derive(Debug)]
pub struct Command<Event, Effect> {
    outputs: SmallVec<[CommandStep<Event, Effect>; 4]>,
}

impl<Event, Effect> Default for Command<Event, Effect> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Event, Effect> Clone for Command<Event, Effect>
where
    Event: Clone,
    Effect: Clone,
{
    fn clone(&self) -> Self {
        let mut outputs = SmallVec::new();
        for o in &self.outputs {
            let cloned = match o {
                CommandStep::Event(e) => CommandStep::Event(e.clone()),
                CommandStep::Effect(fx) => CommandStep::Effect(fx.clone()),
                CommandStep::Batch(v) => CommandStep::Batch(v.clone()),
                CommandStep::Group { effects, mode, barrier, timeout_per } => CommandStep::Group { effects: effects.clone(), mode: *mode, barrier: barrier.clone(), timeout_per: *timeout_per },
                CommandStep::Merge { effects, barrier_event } => CommandStep::Merge { effects: effects.clone(), barrier_event: barrier_event.clone() },
                CommandStep::Join { effects, timeout_per, barrier_event } => CommandStep::Join { effects: effects.clone(), timeout_per: *timeout_per, barrier_event: barrier_event.clone() },
                CommandStep::Race { effects, timeout_per, barrier_event } => CommandStep::Race { effects: effects.clone(), timeout_per: *timeout_per, barrier_event: barrier_event.clone() },
                CommandStep::Chain { effects, barrier_event } => CommandStep::Chain { effects: effects.clone(), barrier_event: barrier_event.clone() },
            };
            outputs.push(cloned);
        }
        Self { outputs }
    }
}

impl<Event, Effect> PartialEq for Command<Event, Effect>
where
    Event: PartialEq,
    Effect: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.outputs == other.outputs
    }
}

impl<Event, Effect> Command<Event, Effect> {
    /// Create a command that merges multiple effect streams in parallel
    ///
    /// Effects run concurrently and their events are dispatched as they arrive.
    /// No barrier event is emitted by default. Use `.barrier_event(event)` to
    /// emit an event when all effects complete.
    pub fn merge(effects: impl IntoIterator<Item = Effect>) -> Self {
        let effects: Vec<Effect> = effects.into_iter().collect();
        if effects.is_empty() {
            return Self::none();
        }
        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Merge { effects, barrier_event: None });
        Self { outputs }
    }

    /// Create a command that joins multiple effect streams in parallel
    ///
    /// Effects run concurrently and their events are dispatched as they arrive.
    /// Use `.barrier_event(event)` to emit an event when all effects complete.
    #[inline]
    pub fn join(effects: impl IntoIterator<Item = Effect>) -> Self {
        let effects: Vec<Effect> = effects.into_iter().collect();
        if effects.is_empty() { return Self::none(); }
        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Join { effects, timeout_per: None, barrier_event: None });
        Self { outputs }
    }

    /// Create a command that races multiple effect streams
    ///
    /// The first effect to produce an event completes the race; other effects
    /// are cancelled. Use `.barrier_event(event)` to emit an event when a winner
    /// is selected.
    pub fn race(effects: impl IntoIterator<Item = Effect>) -> Self {
        let effects: Vec<Effect> = effects.into_iter().collect();
        if effects.is_empty() {
            return Self::none();
        }
        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Race { effects, timeout_per: None, barrier_event: None });
        Self { outputs }
    }

    /// Create a command that chains effects sequentially
    ///
    /// Effects execute one after another; subsequent effects start only after
    /// the previous effect completes. Use `.barrier_event(event)` to emit an
    /// event when the chain completes.
    pub fn chain(effects: impl IntoIterator<Item = Effect>) -> Self {
        let effects: Vec<Effect> = effects.into_iter().collect();
        if effects.is_empty() {
            return Self::none();
        }
        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Chain { effects, barrier_event: None });
        Self { outputs }
    }

    /// Create a command that tries to join multiple effect streams
    ///
    /// Currently behaves the same as `join()`. Short-circuit-on-failure semantics
    /// require an application-specific failure classification and may be layered
    /// via event handling. This placeholder exists for API completeness.
    #[inline]
    // TryJoin removed by design to avoid complexity debt.

    /// Set a per-effect timeout on the most recent group step
    ///
    /// This modifies the last `Group` step in the command. If the command has
    /// no group step, this is a no-op.
    pub fn timeout_per(mut self, duration: Duration) -> Self {
        if let Some(last) = self.outputs.last_mut() {
            match last {
                CommandStep::Group { timeout_per, .. } => *timeout_per = Some(duration),
                CommandStep::Join { timeout_per, .. } => *timeout_per = Some(duration),
                CommandStep::Race { timeout_per, .. } => *timeout_per = Some(duration),
                
                _ => {}
            }
        }
        self
    }

    /// Set a barrier event emitted upon completion of the most recent step
    ///
    /// - For `Group` steps, sets the `barrier` event to dispatch when the
    ///   group completes (all done for parallel/join, first done for race).
    /// - For `Batch` steps (sequential chains), appends the barrier event as a
    ///   subsequent `Event` step so it runs after the chain completes.
    pub fn barrier_event(mut self, event: Event) -> Self {
        match self.outputs.last_mut() {
            Some(CommandStep::Group { barrier, .. }) => { *barrier = Some(event); }
            Some(CommandStep::Merge { barrier_event, .. }) => { *barrier_event = Some(event); }
            Some(CommandStep::Join { barrier_event, .. }) => { *barrier_event = Some(event); }
            Some(CommandStep::Race { barrier_event, .. }) => { *barrier_event = Some(event); }
            
            Some(CommandStep::Chain { barrier_event, .. }) => { *barrier_event = Some(event); }
            Some(CommandStep::Batch(_)) => { self.outputs.push(CommandStep::Event(event)); }
            _ => { self.outputs.push(CommandStep::Event(event)); }
        }
        self
    }
    /// Create a no-op command that produces no outputs
    #[must_use]
    pub fn none() -> Self {
        Self {
            outputs: SmallVec::new(),
        }
    }

    /// Create a command that emits a single event
    pub fn event(event: impl Into<Event>) -> Self {
        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Event(event.into()));
        Self { outputs }
    }

    /// Create a command that requests a single effect
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Clone)] enum Effect { Log(String) }
    /// let command = Command::<(), Effect>::effect(Effect::Log("Hello".to_string()));
    /// ```
    pub fn effect(effect: impl Into<Effect>) -> Self {
        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Effect(effect.into()));
        Self { outputs }
    }

    /// Create a command that emits multiple events
    pub fn events(events: impl IntoIterator<Item = Event>) -> Self {
        let outputs = events.into_iter().map(CommandStep::Event).collect();
        Self { outputs }
    }

    // Sequential effects removed: use `sequence([Command::effect(...), ...])` for sequential effects
    // Parallel effects removed: use `effects([...])` for parallel effects

    /// Combine multiple commands into one
    ///
    /// All outputs from the provided commands will be executed.
    /// Effects will run in parallel (default Shell behavior).
    /// Events will be processed sequentially in the order received.
    ///
    /// # Example
    /// ```rust
    /// # use syzygy::prelude::*;
    /// # #[derive(Debug, Clone)] enum Event { A, B }
    /// # #[derive(Debug, Clone)] enum Effect { X, Y }
    /// let command = Command::batch([
    ///     Command::<Event, Effect>::event(Event::A),
    ///     Command::<Event, Effect>::effect(Effect::X),
    ///     Command::<Event, Effect>::event(Event::B),
    ///     Command::<Event, Effect>::effect(Effect::Y),
    /// ]);
    /// ```
    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        let mut outputs = SmallVec::new();
        for command in commands {
            outputs.extend(command.outputs);
        }
        Self { outputs }
    }

    /// Optimize command by flattening and deduplicating similar effect types
    ///
    /// This optimization consolidates adjacent effects of the same coordination type
    /// (batch or parallel) to reduce executor overhead. For example, multiple
    /// single effect steps are merged into a single Batch step.
    ///
    /// # Example
    /// ```rust,ignore
    /// let cmd = Command::batch([
    ///     Command::effect(Effect::A),    // Individual effect
    ///     Command::effect(Effect::B),    // Individual effect  
    ///     Effects::new([Effect::C, Effect::D]).parallel().spawn(),  // Parallel effects
    ///     Command::effect(Effect::E),    // Individual effect
    /// ]);
    ///
    /// let optimized = cmd.flatten();
    /// // Results in: Group { effects: [A, B, C, D, E], mode: Parallel, barrier: None } - all merged for parallel execution
    /// ```
    #[must_use]
    pub fn flatten(self) -> Self {
        let mut events = Vec::new();
        let mut parallel_effects = Vec::new();
        let mut batch_effects = Vec::new();
        let mut has_batch = false;
        let mut groups: Vec<CommandStep<Event, Effect>> = Vec::new();

        // Collect all outputs by type, merging similar coordination patterns
        for output in self.outputs {
            match output {
                CommandStep::Event(event) => {
                    events.push(event);
                }
                CommandStep::Effect(effect) => {
                    // Individual effects default to parallel execution in Shell
                    parallel_effects.push(effect);
                }
                CommandStep::Group { effects, mode: GroupMode::Parallel, barrier: None, timeout_per: None }
                | CommandStep::Merge { effects, barrier_event: None } => {
                    parallel_effects.extend(effects);
                }
                CommandStep::Batch(effects) => {
                    batch_effects.extend(effects);
                    has_batch = true;
                }
                CommandStep::Group { .. }
                | CommandStep::Merge { barrier_event: Some(_), .. }
                | CommandStep::Join { .. }
                | CommandStep::Race { .. }
                | CommandStep::Chain { .. } => {
                    // Preserve groups as they encode policy/barrier explicitly
                    groups.push(output);
                }
            }
        }

        // Rebuild optimized command
        let mut optimized_outputs = SmallVec::new();

        // Add events first (they execute immediately)
        for event in events {
            optimized_outputs.push(CommandStep::Event(event));
        }

        // Add effects based on coordination requirements
        match (
            has_batch,
            !parallel_effects.is_empty(),
            !batch_effects.is_empty(),
        ) {
            // Only parallel effects
            (false, true, false) => {
                optimized_outputs.push(CommandStep::Merge { effects: parallel_effects, barrier_event: None });
            }
            // Only batch effects
            (true, false, true) => {
                optimized_outputs.push(CommandStep::Batch(batch_effects));
            }
            // Both types - batch takes precedence to maintain ordering
            (true, true, true) => {
                // Merge all effects into batch to preserve ordering guarantees
                batch_effects.extend(parallel_effects);
                optimized_outputs.push(CommandStep::Batch(batch_effects));
            }
            // Mixed without explicit batch - use parallel
            (false, true, true) => {
                parallel_effects.extend(batch_effects);
                optimized_outputs.push(CommandStep::Merge { effects: parallel_effects, barrier_event: None });
            }
            _ => {
                // No effects to optimize
            }
        }

        Self {
            outputs: {
                // Keep optimized outputs first (events/effects), then preserved groups
                optimized_outputs.extend(groups);
                optimized_outputs
            },
        }
    }

    /// Append another command to this one
    #[must_use]
    pub fn append(mut self, other: Self) -> Self {
        self.outputs.extend(other.outputs);
        self
    }

    /// Extend this command with additional outputs
    #[must_use]
    pub fn extend(mut self, outputs: impl IntoIterator<Item = CommandStep<Event, Effect>>) -> Self {
        self.outputs.extend(outputs);
        self
    }

    /// Transform all events in this command
    pub fn map_event<NewEvent, F>(self, mut f: F) -> Command<NewEvent, Effect>
    where
        F: FnMut(Event) -> NewEvent,
    {
        let mut outputs = SmallVec::new();
        for output in self.outputs {
            match output {
                CommandStep::Event(event) => outputs.push(CommandStep::Event(f(event))),
                CommandStep::Effect(effect) => outputs.push(CommandStep::Effect(effect)),
                CommandStep::Batch(effects) => {
                    outputs.push(CommandStep::Batch(effects));
                }
                CommandStep::Group { effects, mode, barrier, timeout_per } => {
                    outputs.push(CommandStep::Group { effects, mode, barrier: barrier.map(&mut f), timeout_per });
                }
                CommandStep::Merge { effects, barrier_event } => outputs.push(CommandStep::Merge { effects, barrier_event: barrier_event.map(&mut f) }),
                CommandStep::Join { effects, timeout_per, barrier_event } => outputs.push(CommandStep::Join { effects, timeout_per, barrier_event: barrier_event.map(&mut f) }),
                CommandStep::Race { effects, timeout_per, barrier_event } => outputs.push(CommandStep::Race { effects, timeout_per, barrier_event: barrier_event.map(&mut f) }),
                CommandStep::Chain { effects, barrier_event } => outputs.push(CommandStep::Chain { effects, barrier_event: barrier_event.map(&mut f) }),
            }
        }
        Command { outputs }
    }

    /// Transform all effects in this command
    pub fn map_effect<NewEffect, F>(self, mut f: F) -> Command<Event, NewEffect>
    where
        F: FnMut(Effect) -> NewEffect,
    {
        let mut outputs = SmallVec::new();

        for output in self.outputs {
            let new_output = match output {
                CommandStep::Event(event) => CommandStep::Event(event),
                CommandStep::Effect(effect) => CommandStep::Effect(f(effect)),
                CommandStep::Batch(effects) => {
                    CommandStep::Batch(effects.into_iter().map(&mut f).collect())
                }
                CommandStep::Group { effects, mode, barrier, timeout_per } => CommandStep::Group { effects: effects.into_iter().map(&mut f).collect(), mode, barrier, timeout_per },
                CommandStep::Merge { effects, barrier_event } => CommandStep::Merge { effects: effects.into_iter().map(&mut f).collect(), barrier_event },
                CommandStep::Join { effects, timeout_per, barrier_event } => CommandStep::Join { effects: effects.into_iter().map(&mut f).collect(), timeout_per, barrier_event },
                CommandStep::Race { effects, timeout_per, barrier_event } => CommandStep::Race { effects: effects.into_iter().map(&mut f).collect(), timeout_per, barrier_event },
                CommandStep::Chain { effects, barrier_event } => CommandStep::Chain { effects: effects.into_iter().map(&mut f).collect(), barrier_event },
            };
            outputs.push(new_output);
        }

        Command { outputs }
    }

    /// Check if this command has any outputs
    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty()
    }

    /// Get the number of logical outputs this command will produce
    ///
    /// Counts inner items for coordinated effects to reflect actual work units.
    pub fn len(&self) -> usize {
        self.outputs
            .iter()
            .map(|o| match o {
                CommandStep::Batch(effects)
                | CommandStep::Group { effects, .. }
                | CommandStep::Merge { effects, .. }
                | CommandStep::Join { effects, .. }
                | CommandStep::Race { effects, .. }
                | CommandStep::Chain { effects, .. } => effects.len(),
                CommandStep::Event(_) | CommandStep::Effect(_) => 1,
            })
            .sum()
    }

    /// Get access to outputs for testing
    pub fn outputs(&self) -> &[CommandStep<Event, Effect>] {
        &self.outputs
    }
}

// Monadic composition methods
impl<Event, Effect> Command<Event, Effect> {
    /// Chain commands based on the outputs of this command
    ///
    /// This enables conditional command chaining where the next command
    /// depends on what outputs the current command produces.
    ///
    /// # Example
    /// ```rust,ignore
    /// let cmd = Command::effect(LoadUser { id: 123 })
    ///     .and_then(|outputs| {
    ///         if outputs.is_empty() {
    ///             Command::event(UserNotFound)
    ///         } else {
    ///             Command::effect(LoadUserPosts { id: 123 })
    ///         }
    ///     });
    /// ```
    #[must_use]
    pub fn and_then<F>(self, f: F) -> Self
    where
        F: FnOnce(&[CommandStep<Event, Effect>]) -> Self,
    {
        let next_command = f(&self.outputs);
        Self::batch([self, next_command])
    }

    /// Execute command only if condition is true
    ///
    /// Returns this command if condition is true, otherwise returns Command::none().
    /// Useful for conditional command execution based on runtime state.
    ///
    /// # Example
    /// ```rust,ignore
    /// let cmd = Command::effect(SaveData { data })
    ///     .when(user_has_permission);
    /// ```
    #[must_use]
    pub fn when(self, condition: bool) -> Self {
        if condition { self } else { Self::none() }
    }

    /// Provide fallback command if this command is empty
    ///
    /// Returns this command if it has any outputs, otherwise returns the fallback.
    /// Useful for providing default behavior when a command produces no outputs.
    ///
    /// # Example
    /// ```rust,ignore
    /// let cmd = Command::none()
    ///     .or_else(Command::event(DefaultAction));
    /// ```
    #[must_use]
    pub fn or_else(self, fallback: Self) -> Self {
        if self.is_empty() { fallback } else { self }
    }

    /// Filter outputs based on predicate
    ///
    /// Keeps only the outputs that match the given predicate.
    /// Useful for selective output processing or removing unwanted outputs.
    ///
    /// # Example
    /// ```rust,ignore
    /// let cmd = Command::batch([
    ///     Command::event(ValidEvent),
    ///     Command::event(ErrorEvent),
    /// ]).filter(|output| !matches!(output, CommandStep::Event(ErrorEvent)));
    /// ```
    #[must_use]
    pub fn filter<F>(self, predicate: F) -> Self
    where
        F: Fn(&CommandStep<Event, Effect>) -> bool,
    {
        let outputs = self
            .outputs
            .into_iter()
            .filter(|output| predicate(output))
            .collect();
        Self { outputs }
    }

    /// Chain commands sequentially
    ///
    /// Equivalent to Command::batch([self, next]) but with clearer sequential intent.
    /// Effects will still execute in parallel - for true sequential execution,
    /// use event-chaining patterns in your update() function.
    ///
    /// # Example
    /// ```rust,ignore
    /// let cmd = Command::effect(StartProcess)
    ///     .then(Command::effect(ProcessStep1))
    ///     .then(Command::event(ProcessComplete));
    /// ```
    #[must_use]
    pub fn then(self, next: Self) -> Self {
        Self::batch([self, next])
    }

}

// Sequential Effect Composition
impl<Event, Effect> Command<Event, Effect> {
    /// Create a sequential effect pipeline
    ///
    /// This creates a special effect marker that tells the Shell to execute
    /// effects sequentially rather than in parallel. The pipeline uses a
    /// simple, type-safe approach that stops on first failure.
    ///
    /// **Grug-friendly solution**: No complex event chains, no state machines,
    /// just readable sequential composition.
    ///
    /// # Example
    /// ```rust,ignore
    /// Command::sequence([
    ///     Command::effect(LoginUser { credentials }),
    ///     Command::effect(FetchUserData { user_id }),
    ///     Command::effect(FetchAddressData { user_id }),
    ///     Command::effect(MakeASandwichForUser { user_id, preferences }),
    /// ])
    /// ```
    ///
    /// This creates a pipeline where each effect waits for the previous one
    /// to complete before executing. Error handling is built-in - any failure
    /// stops the entire pipeline.
    pub fn sequence<T>(items: impl IntoIterator<Item = T>) -> Self
    where
        T: Into<Command<Event, Effect>>,
    {
        let commands: Vec<Self> = items.into_iter().map(Into::into).collect();
        if commands.is_empty() {
            return Self::none();
        }

        // Extract all effects from commands and ensure sequential execution
        let mut all_effects = Vec::new();
        let mut all_events = Vec::new();

        // Flatten all effects into a single list, preserving order
        for command in commands {
            for output in command.outputs {
                match output {
                    CommandStep::Effect(effect) => {
                        all_effects.push(effect);
                    }
                    CommandStep::Batch(effects) => {
                        all_effects.extend(effects);
                    }
                    CommandStep::Group { effects, .. }
                    | CommandStep::Merge { effects, .. }
                    | CommandStep::Join { effects, .. }
                    | CommandStep::Race { effects, .. }
                    | CommandStep::Chain { effects, .. } => {
                        // Grouped effects become sequential in a sequence pipeline  
                        all_effects.extend(effects);
                    }
                    CommandStep::Event(event) => {
                        all_events.push(event);
                    }
                }
            }
        }

        let mut outputs = SmallVec::new();

        // Add events first (they execute immediately)
        for event in all_events {
            outputs.push(CommandStep::Event(event));
        }

        // Then add effects as a batch (enforces ordering via Shell)
        if !all_effects.is_empty() {
            outputs.push(CommandStep::Batch(all_effects));
        }

        Self { outputs }
    }
}

// Domain-specific collection operations
impl<Event, Effect> Command<Event, Effect> {
    /// Partition outputs into separate collections by type
    ///
    /// This is more efficient and type-safe than generic Iterator::partition
    /// as it knows the structure of CommandStep.
    ///
    /// # Example
    /// ```rust,ignore
    /// let (events, effects) = command.partition_outputs();
    /// process_events(events);
    /// execute_effects(effects);
    /// ```
    pub fn partition_outputs(self) -> (Vec<Event>, Vec<Effect>) {
        let mut events = Vec::new();
        let mut effects = Vec::new();

        for output in self.outputs {
            match output {
                CommandStep::Event(event) => events.push(event),
                CommandStep::Effect(effect) => effects.push(effect),
                CommandStep::Batch(coordinated_effects)
                | CommandStep::Group { effects: coordinated_effects, .. }
                | CommandStep::Merge { effects: coordinated_effects, .. }
                | CommandStep::Join { effects: coordinated_effects, .. }
                | CommandStep::Race { effects: coordinated_effects, .. }
                | CommandStep::Chain { effects: coordinated_effects, .. } => {
                    effects.extend(coordinated_effects);
                }
            }
        }

        (events, effects)
    }

    /// Count the number of events in this command
    ///
    /// Useful for understanding command composition and testing.
    pub fn count_events(&self) -> usize {
        self.outputs
            .iter()
            .filter(|output| matches!(output, CommandStep::Event(_)))
            .count()
    }

    /// Count the number of effects in this command
    ///
    /// Useful for understanding command composition and testing.
    pub fn count_effects(&self) -> usize {
        let mut count = 0;
        for output in &self.outputs {
            match output {
                CommandStep::Effect(_) => count += 1,
                CommandStep::Batch(effects)
                | CommandStep::Group { effects, .. }
                | CommandStep::Merge { effects, .. }
                | CommandStep::Join { effects, .. }
                | CommandStep::Race { effects, .. }
                | CommandStep::Chain { effects, .. } => {
                    count += effects.len();
                }
                CommandStep::Event(_) => {}
            }
        }
        count
    }

    /// Find the first event matching a predicate
    ///
    /// Type-safe alternative to generic Iterator methods that preserves
    /// the Event/Effect distinction.
    ///
    /// # Example
    /// ```rust,ignore
    /// if let Some(user_event) = command.find_event(|e| matches!(e, UserEvent::_)) {
    ///     handle_user_event(user_event);
    /// }
    /// ```
    pub fn find_event<F>(&self, predicate: F) -> Option<&Event>
    where
        F: Fn(&Event) -> bool,
    {
        for output in &self.outputs {
            if let CommandStep::Event(event) = output
                && predicate(event)
            {
                return Some(event);
            }
        }
        None
    }

    /// Find the first effect matching a predicate
    ///
    /// Type-safe alternative to generic Iterator methods that preserves
    /// the Event/Effect distinction.
    pub fn find_effect<F>(&self, predicate: F) -> Option<&Effect>
    where
        F: Fn(&Effect) -> bool,
    {
        for output in &self.outputs {
            if let CommandStep::Effect(effect) = output
                && predicate(effect)
            {
                return Some(effect);
            }
        }
        None
    }

    /// Transform events with early termination on error
    ///
    /// This provides a type-safe, fail-fast transformation that stops
    /// at the first error, unlike Iterator::map which would collect
    /// all errors.
    ///
    /// # Example
    /// ```rust,ignore
    /// let validated_cmd = command.try_map_event(|event| {
    ///     validate_event(event).map(|e| ProcessedEvent(e))
    /// })?;
    /// ```
    pub fn try_map_event<NewEvent, E, F>(self, mut f: F) -> Result<Command<NewEvent, Effect>, E>
    where
        F: FnMut(Event) -> Result<NewEvent, E>,
    {
        let mut outputs = SmallVec::new();

        for output in self.outputs {
            match output {
                CommandStep::Event(event) => outputs.push(CommandStep::Event(f(event)?)),
                CommandStep::Effect(effect) => outputs.push(CommandStep::Effect(effect)),
                CommandStep::Batch(effects) => outputs.push(CommandStep::Batch(effects)),
                CommandStep::Group { effects, mode: GroupMode::Parallel, barrier: None, timeout_per: None } => outputs.push(CommandStep::Group { effects, mode: GroupMode::Parallel, barrier: None, timeout_per: None }),
                CommandStep::Group { effects, mode, barrier, timeout_per } => {
                    let mapped = match barrier { Some(ev) => Some(f(ev)?), None => None };
                    outputs.push(CommandStep::Group { effects, mode, barrier: mapped, timeout_per });
                }
                CommandStep::Merge { effects, barrier_event } => outputs.push(CommandStep::Merge { effects, barrier_event: match barrier_event { Some(ev) => Some(f(ev)?), None => None } }),
                CommandStep::Join { effects, timeout_per, barrier_event } => outputs.push(CommandStep::Join { effects, timeout_per, barrier_event: match barrier_event { Some(ev) => Some(f(ev)?), None => None } }),
                CommandStep::Race { effects, timeout_per, barrier_event } => outputs.push(CommandStep::Race { effects, timeout_per, barrier_event: match barrier_event { Some(ev) => Some(f(ev)?), None => None } }),
                CommandStep::Chain { effects, barrier_event } => outputs.push(CommandStep::Chain { effects, barrier_event: match barrier_event { Some(ev) => Some(f(ev)?), None => None } }),
            }
        }

        Ok(Command { outputs })
    }

    /// Transform effects with early termination on error
    ///
    /// Similar to try_map_event but for effects.
    pub fn try_map_effect<NewEffect, E, F>(self, mut f: F) -> Result<Command<Event, NewEffect>, E>
    where
        F: FnMut(Effect) -> Result<NewEffect, E>,
    {
        let mut outputs = SmallVec::new();

        for output in self.outputs {
            let new_output = match output {
                CommandStep::Event(event) => CommandStep::Event(event),
                CommandStep::Effect(effect) => CommandStep::Effect(f(effect)?),
                CommandStep::Batch(effects) => {
                    let mapped_effects: Result<Vec<_>, _> =
                        effects.into_iter().map(&mut f).collect();
                    CommandStep::Batch(mapped_effects?)
                }
                CommandStep::Group { effects, mode, barrier, timeout_per } => {
                    let mapped_effects: Result<Vec<_>, _> = effects.into_iter().map(&mut f).collect();
                    CommandStep::Group { effects: mapped_effects?, mode, barrier, timeout_per }
                }
                CommandStep::Merge { effects, barrier_event } => {
                    let mapped_effects: Result<Vec<_>, _> = effects.into_iter().map(&mut f).collect();
                    CommandStep::Merge { effects: mapped_effects?, barrier_event }
                }
                CommandStep::Join { effects, timeout_per, barrier_event } => {
                    let mapped_effects: Result<Vec<_>, _> = effects.into_iter().map(&mut f).collect();
                    CommandStep::Join { effects: mapped_effects?, timeout_per, barrier_event }
                }
                CommandStep::Race { effects, timeout_per, barrier_event } => {
                    let mapped_effects: Result<Vec<_>, _> = effects.into_iter().map(&mut f).collect();
                    CommandStep::Race { effects: mapped_effects?, timeout_per, barrier_event }
                }
                CommandStep::Chain { effects, barrier_event } => {
                    let mapped_effects: Result<Vec<_>, _> = effects.into_iter().map(&mut f).collect();
                    CommandStep::Chain { effects: mapped_effects?, barrier_event }
                }
            };
            outputs.push(new_output);
        }

        Ok(Command { outputs })
    }

    /// Get read-only slice access to outputs without consuming the command
    ///
    /// Useful for inspection and debugging without losing ownership.
    /// This provides the Iterator-like access pattern when needed without
    /// implementing the full Iterator trait.
    pub fn iter(&self) -> std::slice::Iter<'_, CommandStep<Event, Effect>> {
        self.outputs.iter()
    }

    /// Check if command contains any events
    pub fn has_events(&self) -> bool {
        self.outputs
            .iter()
            .any(|output| matches!(output, CommandStep::Event(_)))
    }

    /// Check if command contains any effects
    pub fn has_effects(&self) -> bool {
        self.outputs.iter().any(|output| {
            matches!(
                output,
                CommandStep::Effect(_) | CommandStep::Batch(_)
            )
        })
    }

    /// Retain only events matching a predicate (in-place filtering)
    ///
    /// This modifies the command in place, keeping only events that match
    /// the predicate while preserving all effects.
    ///
    /// # Example
    /// ```rust,ignore
    /// command.retain_events(|event| matches!(event, ImportantEvent::_));
    /// ```
    pub fn retain_events<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&Event) -> bool,
    {
        self.outputs.retain(|output| match output {
            CommandStep::Event(event) => predicate(event),
            CommandStep::Effect(_)
            | CommandStep::Batch(_)
            | CommandStep::Group { .. }
            | CommandStep::Merge { .. }
            | CommandStep::Join { .. }
            | CommandStep::Race { .. }
            | CommandStep::Chain { .. } => true, // Always keep effects and coordination
        });
    }

    /// Retain only effects matching a predicate (in-place filtering)
    ///
    /// This modifies the command in place, keeping only effects that match
    /// the predicate while preserving all events.
    ///
    /// # Example
    /// ```rust,ignore
    /// command.retain_effects(|effect| matches!(effect, CriticalEffect::_));
    /// ```
    pub fn retain_effects<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&Effect) -> bool,
    {
        self.outputs.retain(|output| match output {
            CommandStep::Effect(effect) => predicate(effect),
            CommandStep::Event(_)
            | CommandStep::Batch(_)
            | CommandStep::Group { .. }
            | CommandStep::Merge { .. }
            | CommandStep::Join { .. }
            | CommandStep::Race { .. }
            | CommandStep::Chain { .. } => true, // Always keep events and coordinated effects
        });
    }

    /// Consume command and return only the events
    ///
    /// This is a typed alternative to `partition_outputs()` when you only
    /// need the events and want to discard effects.
    ///
    /// # Example
    /// ```rust,ignore
    /// let events: Vec<Event> = command.into_events();
    /// process_events(events);
    /// ```
    pub fn into_events(self) -> Vec<Event> {
        let mut events = Vec::new();
        for output in self.outputs {
            if let CommandStep::Event(event) = output {
                events.push(event);
            }
        }
        events
    }

    /// Consume command and return only the effects
    ///
    /// This is a typed alternative to `partition_outputs()` when you only
    /// need the effects and want to discard events.
    ///
    /// # Example
    /// ```rust,ignore
    /// let effects: Vec<Effect> = command.into_effects();
    /// execute_effects(effects);
    /// ```
    pub fn into_effects(self) -> Vec<Effect> {
        let mut effects = Vec::new();
        for output in self.outputs {
            match output {
                CommandStep::Effect(effect) => effects.push(effect),
                CommandStep::Batch(coordinated_effects)
                | CommandStep::Group { effects: coordinated_effects, .. }
                | CommandStep::Merge { effects: coordinated_effects, .. }
                | CommandStep::Join { effects: coordinated_effects, .. }
                | CommandStep::Race { effects: coordinated_effects, .. }
                | CommandStep::Chain { effects: coordinated_effects, .. } => {
                    effects.extend(coordinated_effects);
                }
                CommandStep::Event(_) => {}
            }
        }
        effects
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum TestEvent {
        A,
        B,
        C,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum TestEffect {
        X,
        Y,
        Z,
    }

    // Helper functions for test code
    fn test_event(event: TestEvent) -> Command<TestEvent, TestEffect> {
        Command::event(event)
    }

    fn test_effect(effect: TestEffect) -> Command<TestEvent, TestEffect> {
        Command::effect(effect)
    }

    #[test]
    fn test_empty_command() {
        let cmd = Command::<TestEvent, TestEffect>::none();
        assert!(cmd.is_empty());
        assert_eq!(cmd.len(), 0);

        let outputs: Vec<_> = cmd.into_iter().collect();
        assert!(outputs.is_empty());
    }

    #[test]
    fn test_single_event() {
        let cmd = Command::<TestEvent, TestEffect>::event(TestEvent::A);
        assert!(!cmd.is_empty());
        assert_eq!(cmd.len(), 1);

        let outputs: Vec<_> = cmd.into_iter().collect();
        assert_eq!(outputs, vec![CommandStep::Event(TestEvent::A)]);
    }

    #[test]
    fn test_single_effect() {
        let cmd = Command::<TestEvent, TestEffect>::effect(TestEffect::X);
        assert_eq!(cmd.len(), 1);

        let outputs: Vec<_> = cmd.into_iter().collect();
        assert_eq!(outputs, vec![CommandStep::Effect(TestEffect::X)]);
    }

    #[test]
    fn test_multiple_events() {
        let cmd =
            Command::<TestEvent, TestEffect>::events([TestEvent::A, TestEvent::B, TestEvent::C]);
        assert_eq!(cmd.len(), 3);

        let outputs: Vec<_> = cmd.into_iter().collect();
        assert_eq!(
            outputs,
            vec![
                CommandStep::Event(TestEvent::A),
                CommandStep::Event(TestEvent::B),
                CommandStep::Event(TestEvent::C),
            ]
        );
    }

    #[test]
    fn test_batch_commands() {
        let cmd1 = test_event(TestEvent::A);
        let cmd2 = test_effect(TestEffect::X);
        let cmd3 = Command::events([TestEvent::B, TestEvent::C]);

        let combined = Command::batch([cmd1, cmd2, cmd3]);
        assert_eq!(combined.len(), 4);

        let outputs: Vec<_> = combined.into_iter().collect();
        assert_eq!(
            outputs,
            vec![
                CommandStep::Event(TestEvent::A),
                CommandStep::Effect(TestEffect::X),
                CommandStep::Event(TestEvent::B),
                CommandStep::Event(TestEvent::C),
            ]
        );
    }

    #[test]
    fn test_map_event() {
        let cmd = Command::<TestEvent, TestEffect>::events([TestEvent::A, TestEvent::B]);

        let mapped = cmd.map_event(|e| match e {
            TestEvent::A => TestEvent::C,
            TestEvent::B => TestEvent::A,
            TestEvent::C => TestEvent::B,
        });

        let outputs: Vec<_> = mapped.into_iter().collect();
        assert_eq!(
            outputs,
            vec![
                CommandStep::Event(TestEvent::C),
                CommandStep::Event(TestEvent::A),
            ]
        );
    }

    #[test]
    fn test_append() {
        let cmd1 = test_event(TestEvent::A);
        let cmd2 = test_effect(TestEffect::X);

        let combined = cmd1.append(cmd2);
        let outputs: Vec<_> = combined.into_iter().collect();

        assert_eq!(
            outputs,
            vec![
                CommandStep::Event(TestEvent::A),
                CommandStep::Effect(TestEffect::X),
            ]
        );
    }

    #[test]
    fn test_smallvec_performance() {
        // Commands with ≤4 outputs should not allocate
        let cmd = Command::batch([
            test_event(TestEvent::A),
            test_event(TestEvent::B),
            test_effect(TestEffect::X),
            test_effect(TestEffect::Y),
        ]);

        assert_eq!(cmd.len(), 4);
        // SmallVec should still be inline (no heap allocation for ≤4 items)
        assert!(!cmd.outputs.spilled());
    }

    // Tests for monadic composition methods

    #[test]
    fn test_and_then() {
        // Test conditional chaining based on command outputs
        let cmd_with_output =
            Command::<TestEvent, TestEffect>::event(TestEvent::A).and_then(|outputs| {
                if outputs.is_empty() {
                    test_event(TestEvent::B)
                } else {
                    test_effect(TestEffect::X)
                }
            });

        assert_eq!(cmd_with_output.len(), 2);
        let outputs: Vec<_> = cmd_with_output.into_iter().collect();
        assert_eq!(
            outputs,
            vec![
                CommandStep::Event(TestEvent::A),
                CommandStep::Effect(TestEffect::X),
            ]
        );

        // Test with empty command
        let empty_cmd = Command::<TestEvent, TestEffect>::none().and_then(|outputs| {
            if outputs.is_empty() {
                test_event(TestEvent::B)
            } else {
                test_effect(TestEffect::X)
            }
        });

        assert_eq!(empty_cmd.len(), 1);
        let outputs: Vec<_> = empty_cmd.into_iter().collect();
        assert_eq!(outputs, vec![CommandStep::Event(TestEvent::B)]);
    }

    #[test]
    fn test_when() {
        let cmd = Command::<TestEvent, TestEffect>::event(TestEvent::A);

        // Test when condition is true
        let when_true = cmd.clone().when(true);
        assert_eq!(when_true.len(), 1);
        let outputs: Vec<_> = when_true.into_iter().collect();
        assert_eq!(outputs, vec![CommandStep::Event(TestEvent::A)]);

        // Test when condition is false
        let when_false = cmd.when(false);
        assert!(when_false.is_empty());
    }

    #[test]
    fn test_or_else() {
        // Test with non-empty command
        let non_empty = Command::<TestEvent, TestEffect>::event(TestEvent::A)
            .or_else(test_effect(TestEffect::X));

        assert_eq!(non_empty.len(), 1);
        let outputs: Vec<_> = non_empty.into_iter().collect();
        assert_eq!(outputs, vec![CommandStep::Event(TestEvent::A)]);

        // Test with empty command
        let empty = Command::<TestEvent, TestEffect>::none().or_else(test_effect(TestEffect::X));

        assert_eq!(empty.len(), 1);
        let outputs: Vec<_> = empty.into_iter().collect();
        assert_eq!(outputs, vec![CommandStep::Effect(TestEffect::X)]);
    }

    #[test]
    fn test_filter() {
        let cmd = Command::<TestEvent, TestEffect>::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
            test_effect(TestEffect::Y),
        ]);

        // Filter to only keep events
        let events_only = cmd
            .clone()
            .filter(|output| matches!(output, CommandStep::Event(_)));

        assert_eq!(events_only.len(), 2);
        let outputs: Vec<_> = events_only.into_iter().collect();
        assert_eq!(
            outputs,
            vec![
                CommandStep::Event(TestEvent::A),
                CommandStep::Event(TestEvent::B),
            ]
        );

        // Filter to only keep effects
        let effects_only = cmd.filter(|output| matches!(output, CommandStep::Effect(_)));

        assert_eq!(effects_only.len(), 2);
        let outputs: Vec<_> = effects_only.into_iter().collect();
        assert_eq!(
            outputs,
            vec![
                CommandStep::Effect(TestEffect::X),
                CommandStep::Effect(TestEffect::Y),
            ]
        );
    }

    #[test]
    fn test_then() {
        let cmd1 = Command::<TestEvent, TestEffect>::event(TestEvent::A);
        let cmd2 = test_effect(TestEffect::X);

        let combined = cmd1.then(cmd2);
        assert_eq!(combined.len(), 2);

        let outputs: Vec<_> = combined.into_iter().collect();
        assert_eq!(
            outputs,
            vec![
                CommandStep::Event(TestEvent::A),
                CommandStep::Effect(TestEffect::X),
            ]
        );
    }

    #[test]
    fn test_monadic_composition_chain() {
        // Test complex chaining of multiple monadic methods
        let complex_cmd = Command::<TestEvent, TestEffect>::batch([
            test_event(TestEvent::A),
            test_event(TestEvent::B),
            test_effect(TestEffect::Z), // This will be filtered out
        ])
        .filter(|output| !matches!(output, CommandStep::Effect(TestEffect::Z)))
        .and_then(|outputs| {
            if outputs.len() >= 2 {
                test_effect(TestEffect::X)
            } else {
                Command::none()
            }
        })
        .when(true)
        .or_else(test_event(TestEvent::C))
        .then(test_effect(TestEffect::Y));

        // Should have: TestEvent::A, TestEvent::B, TestEffect::X, TestEffect::Y
        assert_eq!(complex_cmd.len(), 4);

        let outputs: Vec<_> = complex_cmd.into_iter().collect();
        assert_eq!(
            outputs,
            vec![
                CommandStep::Event(TestEvent::A),
                CommandStep::Event(TestEvent::B),
                CommandStep::Effect(TestEffect::X),
                CommandStep::Effect(TestEffect::Y),
            ]
        );
    }

    #[test]
    fn test_monadic_methods_preserve_smallvec_optimization() {
        // Test that monadic methods don't break SmallVec inline optimization
        let cmd = Command::<TestEvent, TestEffect>::event(TestEvent::A)
            .then(test_event(TestEvent::B))
            .then(test_effect(TestEffect::X))
            .when(true);

        assert_eq!(cmd.len(), 3);
        // Should still be inline (≤4 items)
        assert!(!cmd.outputs.spilled());
    }

    #[test]
    fn test_partition_outputs() {
        let command = Command::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
            test_effect(TestEffect::Y),
        ]);

        let (events, effects) = command.partition_outputs();

        assert_eq!(events.len(), 2);
        assert_eq!(effects.len(), 2);
        assert_eq!(events, vec![TestEvent::A, TestEvent::B]);
        assert_eq!(effects, vec![TestEffect::X, TestEffect::Y]);
    }

    #[test]
    fn test_count_events_and_effects() {
        let command = Command::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
        ]);

        assert_eq!(command.count_events(), 2);
        assert_eq!(command.count_effects(), 1);

        let empty_command = Command::<TestEvent, TestEffect>::none();
        assert_eq!(empty_command.count_events(), 0);
        assert_eq!(empty_command.count_effects(), 0);
    }

    #[test]
    fn test_find_event_and_effect() {
        let command = Command::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
        ]);

        // Find existing event
        let found_event = command.find_event(|e| matches!(e, TestEvent::A));
        assert_eq!(found_event, Some(&TestEvent::A));

        // Find non-existing event
        let not_found = command.find_event(|e| matches!(e, TestEvent::C));
        assert_eq!(not_found, None);

        // Find existing effect
        let found_effect = command.find_effect(|e| matches!(e, TestEffect::X));
        assert_eq!(found_effect, Some(&TestEffect::X));

        // Find non-existing effect
        let not_found_effect = command.find_effect(|e| matches!(e, TestEffect::Y));
        assert_eq!(not_found_effect, None);
    }

    #[test]
    fn test_try_map_event_success() {
        let command = Command::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
        ]);

        let result: Result<Command<TestEvent, TestEffect>, &str> =
            command.try_map_event(|event| match event {
                TestEvent::A => Ok(TestEvent::B),
                TestEvent::B => Ok(TestEvent::C),
                TestEvent::C => Ok(event),
            });

        assert!(result.is_ok());
        let mapped_command = result.unwrap();
        assert_eq!(mapped_command.count_events(), 2);
        assert_eq!(mapped_command.count_effects(), 1); // Effect unchanged

        let (events, effects) = mapped_command.partition_outputs();
        assert_eq!(events, vec![TestEvent::B, TestEvent::C]);
        assert_eq!(effects, vec![TestEffect::X]);
    }

    #[test]
    fn test_try_map_event_failure() {
        let command: Command<TestEvent, TestEffect> =
            Command::batch([test_event(TestEvent::A), test_event(TestEvent::B)]);

        let result: Result<Command<TestEvent, TestEffect>, &str> =
            command.try_map_event(|event| match event {
                TestEvent::A => Ok(TestEvent::C),
                TestEvent::B => Err("Cannot map B"),
                TestEvent::C => Ok(event),
            });

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Cannot map B");
    }

    #[test]
    fn test_iter_non_consuming() {
        let command = Command::batch([test_event(TestEvent::A), test_effect(TestEffect::X)]);

        // Use iter() without consuming
        let count = command.iter().count();
        assert_eq!(count, 2);

        // Command should still be usable
        assert_eq!(command.len(), 2);
        assert!(command.has_events());
        assert!(command.has_effects());
    }

    #[test]
    fn test_has_events_and_effects() {
        let events_only: Command<TestEvent, TestEffect> = test_event(TestEvent::A);
        assert!(events_only.has_events());
        assert!(!events_only.has_effects());

        let effects_only: Command<TestEvent, TestEffect> = test_effect(TestEffect::X);
        assert!(!effects_only.has_events());
        assert!(effects_only.has_effects());

        let mixed = Command::batch([test_event(TestEvent::A), test_effect(TestEffect::X)]);
        assert!(mixed.has_events());
        assert!(mixed.has_effects());

        let empty = Command::<TestEvent, TestEffect>::none();
        assert!(!empty.has_events());
        assert!(!empty.has_effects());
    }

    #[test]
    fn test_retain_events() {
        let mut command = Command::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
            test_effect(TestEffect::Y),
        ]);

        // Retain only TestEvent::A
        command.retain_events(|e| matches!(e, TestEvent::A));

        assert_eq!(command.count_events(), 1);
        assert_eq!(command.count_effects(), 2); // Effects should be preserved

        let (events, effects) = command.partition_outputs();
        assert_eq!(events, vec![TestEvent::A]);
        assert_eq!(effects, vec![TestEffect::X, TestEffect::Y]);
    }

    #[test]
    fn test_retain_effects() {
        let mut command = Command::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
            test_effect(TestEffect::Y),
        ]);

        // Retain only TestEffect::X
        command.retain_effects(|e| matches!(e, TestEffect::X));

        assert_eq!(command.count_events(), 2); // Events should be preserved
        assert_eq!(command.count_effects(), 1);

        let (events, effects) = command.partition_outputs();
        assert_eq!(events, vec![TestEvent::A, TestEvent::B]);
        assert_eq!(effects, vec![TestEffect::X]);
    }

    #[test]
    fn test_into_events() {
        let command = Command::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
        ]);

        let events = command.into_events();
        assert_eq!(events, vec![TestEvent::A, TestEvent::B]);
    }

    #[test]
    fn test_into_effects() {
        let command = Command::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_effect(TestEffect::Y),
        ]);

        let effects = command.into_effects();
        assert_eq!(effects, vec![TestEffect::X, TestEffect::Y]);
    }

    #[test]
    fn test_into_events_empty() {
        let command = Command::batch([test_effect(TestEffect::X), test_effect(TestEffect::Y)]);

        let events: Vec<TestEvent> = command.into_events();
        assert!(events.is_empty());
    }

    #[test]
    fn test_into_effects_empty() {
        let command = Command::batch([test_event(TestEvent::A), test_event(TestEvent::B)]);

        let effects: Vec<TestEffect> = command.into_effects();
        assert!(effects.is_empty());
    }

    #[test]
    fn test_into_iterator_for_reference() {
        let command = Command::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
        ]);

        // Test &Command iteration without consuming
        let mut count = 0;
        for output in &command {
            match output {
                CommandStep::Event(_) => count += 1,
                CommandStep::Effect(_) => count += 10,
                CommandStep::Batch(_) => count += 100,
                CommandStep::Group { mode: GroupMode::Parallel, barrier: None, .. }
                | CommandStep::Merge { barrier_event: None, .. } => count += 200,
                _ => count += 300,
            }
        }
        assert_eq!(count, 12); // 2 events + 1 effect = 1 + 10 + 1 = 12

        // Command should still be usable after iteration
        assert_eq!(command.len(), 3);
        assert_eq!(command.count_events(), 2);
        assert_eq!(command.count_effects(), 1);

        // Can iterate again
        let outputs: Vec<_> = (&command).into_iter().collect();
        assert_eq!(outputs.len(), 3);
    }

    // Tests for sequential effect composition

    #[test]
    fn test_sequence_empty() {
        let empty: Vec<Command<TestEvent, TestEffect>> = Vec::new();
        let cmd = Command::<TestEvent, TestEffect>::sequence(empty);
        assert!(cmd.is_empty());
        assert_eq!(cmd.len(), 0);
    }

    #[test]
    fn test_sequence_single_effect() {
        let cmd = Command::<TestEvent, TestEffect>::sequence([test_effect(TestEffect::X)]);

        assert_eq!(cmd.len(), 1);
        assert!(cmd.has_effects());
        assert!(!cmd.has_events());

        let effects = cmd.into_effects();
        assert_eq!(effects, vec![TestEffect::X]);
    }

    #[test]
    fn test_sequence_multiple_effects() {
        let cmd = Command::<TestEvent, TestEffect>::sequence([
            test_effect(TestEffect::X),
            test_effect(TestEffect::Y),
            test_effect(TestEffect::Z),
        ]);

        assert_eq!(cmd.len(), 3);
        assert_eq!(cmd.count_effects(), 3);
        assert_eq!(cmd.count_events(), 0);

        let effects = cmd.into_effects();
        assert_eq!(effects, vec![TestEffect::X, TestEffect::Y, TestEffect::Z]);
    }

    #[test]
    fn test_sequence_mixed_commands() {
        let cmd = Command::<TestEvent, TestEffect>::sequence([
            test_effect(TestEffect::X),
            Command::batch([test_event(TestEvent::A), test_effect(TestEffect::Y)]),
            test_effect(TestEffect::Z),
        ]);

        assert_eq!(cmd.len(), 4); // X, A, Y, Z
        assert_eq!(cmd.count_effects(), 3);
        assert_eq!(cmd.count_events(), 1);

        let (events, effects) = cmd.partition_outputs();
        assert_eq!(events, vec![TestEvent::A]);
        assert_eq!(effects, vec![TestEffect::X, TestEffect::Y, TestEffect::Z]);
    }

    #[test]
    fn test_sequence_mixed_items_into_command() {
        // Mix events and effects directly as Commands
        let cmd = Command::<TestEvent, TestEffect>::sequence([
            test_effect(TestEffect::X),
            test_event(TestEvent::A),
            test_effect(TestEffect::Y),
            test_effect(TestEffect::Z),
        ]);

        assert_eq!(cmd.len(), 4); // X, A, Y, Z
        assert_eq!(cmd.count_effects(), 3);
        assert_eq!(cmd.count_events(), 1);

        let (events, effects) = cmd.partition_outputs();
        assert_eq!(events, vec![TestEvent::A]);
        assert_eq!(effects, vec![TestEffect::X, TestEffect::Y, TestEffect::Z]);
    }

    // pipeline builder removed; use Command::sequence instead

    #[test]
    fn test_sequence_preserves_smallvec_optimization() {
        // Test that sequences with ≤4 effects stay inline
        let cmd = Command::<TestEvent, TestEffect>::sequence([
            test_effect(TestEffect::X),
            test_effect(TestEffect::Y),
            test_effect(TestEffect::Z),
        ]);

        assert_eq!(cmd.len(), 3);
        // Should still be inline (≤4 items)
        assert!(!cmd.outputs.spilled());
    }

    #[test]
    fn test_sequence_integration_with_monadic_methods() {
        // Test that sequential effects work well with existing monadic composition
        let cmd = Command::<TestEvent, TestEffect>::sequence([
            test_effect(TestEffect::X),
            test_effect(TestEffect::Y),
        ])
        .then(test_event(TestEvent::A))
        .when(true)
        .or_else(test_effect(TestEffect::Z));

        assert_eq!(cmd.len(), 3); // X, Y, A
        assert_eq!(cmd.count_effects(), 2);
        assert_eq!(cmd.count_events(), 1);

        let (events, effects) = cmd.partition_outputs();
        assert_eq!(events, vec![TestEvent::A]);
        assert_eq!(effects, vec![TestEffect::X, TestEffect::Y]);
    }

    // Tests for unified coordination helpers

    #[test]
    fn test_merge_barrier() {
        let cmd = Command::<TestEvent, TestEffect>::merge([TestEffect::X, TestEffect::Y])
            .barrier_event(TestEvent::A);

        let outputs: Vec<_> = cmd.into_iter().collect();
        assert_eq!(outputs.len(), 1);
        match &outputs[0] {
            CommandStep::Merge { effects, barrier_event } => {
                assert_eq!(effects, &vec![TestEffect::X, TestEffect::Y]);
                assert_eq!(*barrier_event, Some(TestEvent::A));
            }
            _ => panic!("Expected Merge step"),
        }
    }

    #[test]
    fn test_join_without_barrier() {
        let cmd = Command::<TestEvent, TestEffect>::join([TestEffect::X, TestEffect::Y]);
        let outputs: Vec<_> = cmd.into_iter().collect();
        assert_eq!(outputs.len(), 1);
        match &outputs[0] {
            CommandStep::Join { effects, timeout_per, barrier_event } => {
                assert_eq!(effects, &vec![TestEffect::X, TestEffect::Y]);
                assert_eq!(*timeout_per, None);
                assert_eq!(*barrier_event, None);
            }
            _ => panic!("Expected Join step"),
        }
    }

    #[test]
    fn test_race_with_barrier() {
        let cmd = Command::<TestEvent, TestEffect>::race([TestEffect::X, TestEffect::Y])
            .barrier_event(TestEvent::B);
        let outputs: Vec<_> = cmd.into_iter().collect();
        assert_eq!(outputs.len(), 1);
        match &outputs[0] {
            CommandStep::Race { effects, barrier_event, .. } => {
                assert_eq!(effects, &vec![TestEffect::X, TestEffect::Y]);
                assert_eq!(*barrier_event, Some(TestEvent::B));
            }
            _ => panic!("Expected Race step"),
        }
    }

    #[test]
    fn test_chain_with_barrier() {
        let cmd = Command::<TestEvent, TestEffect>::chain([TestEffect::X, TestEffect::Y])
            .barrier_event(TestEvent::C);
        let outputs: Vec<_> = cmd.into_iter().collect();
        assert_eq!(outputs.len(), 1);
        match &outputs[0] {
            CommandStep::Chain { effects, barrier_event } => {
                assert_eq!(effects, &vec![TestEffect::X, TestEffect::Y]);
                assert_eq!(*barrier_event, Some(TestEvent::C));
            }
            _ => panic!("Expected Chain step"),
        }
    }

    

    // Tests for command flattening optimization

    #[test]
    fn test_flatten_empty_command() {
        let cmd = Command::<TestEvent, TestEffect>::none();
        let flattened = cmd.flatten();
        assert!(flattened.is_empty());
    }

    #[test]
    fn test_flatten_only_events() {
        let cmd = Command::<TestEvent, TestEffect>::events([TestEvent::A, TestEvent::B]);
        let flattened = cmd.flatten();

        assert_eq!(flattened.len(), 2);
        assert_eq!(flattened.count_events(), 2);
        assert_eq!(flattened.count_effects(), 0);

        let events = flattened.into_events();
        assert_eq!(events, vec![TestEvent::A, TestEvent::B]);
    }

    #[test]
    fn test_flatten_individual_effects_to_parallel() {
        let cmd = Command::<TestEvent, TestEffect>::batch([
            test_effect(TestEffect::X),
            test_effect(TestEffect::Y),
            test_effect(TestEffect::Z),
        ]);
        let flattened = cmd.flatten();

        assert_eq!(flattened.len(), 3);
        assert_eq!(flattened.count_effects(), 3);

        // Should be consolidated into a single Group step  
        let outputs: Vec<_> = flattened.into_iter().collect();
        assert_eq!(outputs.len(), 1);

        match &outputs[0] {
            CommandStep::Merge { effects, barrier_event: None } => {
                assert_eq!(effects, &vec![TestEffect::X, TestEffect::Y, TestEffect::Z]);
            }
            _ => panic!("Expected Merge step"),
        }
    }

    #[test]
    fn test_flatten_mixed_parallel_effects() {
        let cmd = Command::<TestEvent, TestEffect>::batch([
            test_effect(TestEffect::X),
            Effects::new([TestEffect::Y, TestEffect::Z]).parallel().spawn(),
        ]);
        let flattened = cmd.flatten();

        assert_eq!(flattened.len(), 3);
        assert_eq!(flattened.count_effects(), 3);

        let effects = flattened.into_effects();
        assert_eq!(effects, vec![TestEffect::X, TestEffect::Y, TestEffect::Z]);
    }

    #[test]
    fn test_flatten_only_batch_effects() {
        let cmd = Command::<TestEvent, TestEffect>::sequence([
            test_effect(TestEffect::X),
            test_effect(TestEffect::Y),
        ]);
        let flattened = cmd.flatten();

        assert_eq!(flattened.len(), 2);
        assert_eq!(flattened.count_effects(), 2);

        // Should maintain sequential coordination
        let outputs: Vec<_> = flattened.into_iter().collect();
        assert_eq!(outputs.len(), 1);

        match &outputs[0] {
            CommandStep::Batch(effects) => {
                assert_eq!(effects, &vec![TestEffect::X, TestEffect::Y]);
            }
            _ => panic!("Expected SequentialEffects"),
        }
    }

    #[test]
    fn test_flatten_mixed_events_and_effects() {
        let cmd = Command::<TestEvent, TestEffect>::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_event(TestEvent::B),
            Effects::new([TestEffect::Y, TestEffect::Z]).parallel().spawn(),
        ]);
        let flattened = cmd.flatten();

        assert_eq!(flattened.len(), 5); // A, B, X, Y, Z
        assert_eq!(flattened.count_events(), 2);
        assert_eq!(flattened.count_effects(), 3);

        let outputs: Vec<_> = flattened.into_iter().collect();
        assert_eq!(outputs.len(), 3); // 2 events + 1 parallel effects group

        // Events should come first
        assert!(matches!(outputs[0], CommandStep::Event(TestEvent::A)));
        assert!(matches!(outputs[1], CommandStep::Event(TestEvent::B)));

        // Effects should be grouped
        match &outputs[2] {
            CommandStep::Merge { effects, barrier_event: None } => {
                assert_eq!(effects, &vec![TestEffect::X, TestEffect::Y, TestEffect::Z]);
            }
            _ => panic!("Expected Merge step"),
        }
    }

    #[test]
    fn test_flatten_sequential_takes_precedence() {
        let cmd = Command::<TestEvent, TestEffect>::batch([
            test_effect(TestEffect::X),                      // Parallel by default
            Command::sequence([test_effect(TestEffect::Y)]), // Sequential
            Effects::new([TestEffect::Z]).parallel().spawn(),              // Parallel
        ]);
        let flattened = cmd.flatten();

        // Clone to test both effects and structure
        let effects = flattened.clone().into_effects();
        assert_eq!(effects, vec![TestEffect::Y, TestEffect::X, TestEffect::Z]);

        let outputs: Vec<_> = flattened.into_iter().collect();
        assert_eq!(outputs.len(), 1);

        match &outputs[0] {
            CommandStep::Batch(_) => {
                // Correct - sequential coordination preserved
            }
            _ => panic!("Expected SequentialEffects when mixing sequential and parallel"),
        }
    }

    #[test]
    fn test_flatten_preserves_smallvec_optimization() {
        let cmd = Command::<TestEvent, TestEffect>::batch([
            test_event(TestEvent::A),
            test_effect(TestEffect::X),
            test_effect(TestEffect::Y),
        ]);
        let flattened = cmd.flatten();

        // Should still be inline after flattening (2 outputs: 1 event + 1 parallel effects)
        assert!(!flattened.outputs.spilled());
        assert_eq!(flattened.outputs.len(), 2);
    }
}
