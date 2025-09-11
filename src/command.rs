use smallvec::SmallVec;



/// One atomic operation in the Core→Shell pipeline.
///
/// This is what actually happens when your event handler returns a Command.
/// Events go back to Core for immediate processing. Effects get queued for
/// async execution. Batch effects run sequentially - no parallelism, no race
/// conditions, no surprises at 3 AM when your app shits itself in production.
#[derive(Clone)]
pub enum CommandStep<Event, Effect> {
    Event(Event),
    Effect(Effect),
    /// Sequential effects; Shell runs them in order
    Batch(Vec<Effect>),
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
    /// fn handle_increment(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     model.counter += 1;
    ///     Command::none() // Model updated, no side effects needed
    /// }
    /// ```
    ///
    /// # Example
    /// ```
    /// fn handle_increment(event: Event, model: &mut Model) -> Command<Event, Effect> {
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

    /// Creates a command that immediately triggers another event.
    ///
    /// The event gets processed synchronously in the same tick. No async
    /// boundary, no delay, no opportunity for the universe to fuck with your
    /// state between events. Use this for event chaining when you need to
    /// break complex logic into smaller, testable pieces.
    ///
    /// # Example
    /// ```
    /// fn handle_login(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     if model.user.is_authenticated() {
    ///         // Chain to dashboard event immediately
    ///         Command::event(Event::ShowDashboard)
    ///     } else {
    ///         Command::effect(Effect::Authenticate { credentials: event.credentials })
    ///     }
    /// }
    /// ```
    pub fn event(event: impl Into<Event>) -> Self {
        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Event(event.into()));
        Self { outputs }
    }

    /// Creates a command that triggers an async side effect.
    ///
    /// Effects run in the Shell's async context. They can spawn tasks, make
    /// HTTP requests, write files - all the dirty stuff your pure event handler
    /// shouldn't touch. Effects can send events back to Core when they're done.
    ///
    /// # Example
    /// ```
    /// fn handle_save(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     let data = model.data.clone();
    ///     Command::effect(Effect::SaveToDisk { data })
    /// }
    /// ```
    pub fn effect(effect: impl Into<Effect>) -> Self {
        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Effect(effect.into()));
        Self { outputs }
    }

    /// Creates a command that fires multiple events in order.
    ///
    /// Events are processed sequentially in the order provided. Each event
    /// gets its own call to your event handler, so the model can change
    /// between events. Use this when you need to trigger a sequence of
    /// state changes without any async operations between them.
    ///
    /// # Example
    /// ```
    /// fn handle_reset(event: Event, model: &mut Model) -> Command<Event, Effect> {
    ///     Command::events(vec![
    ///         Event::ClearUserData,
    ///         Event::ResetUI,
    ///         Event::ShowWelcomeScreen,
    ///     ])
    /// }
    /// ```
    pub fn events(events: impl IntoIterator<Item = Event>) -> Self {
        let outputs = events.into_iter().map(CommandStep::Event).collect();
        Self { outputs }
    }

    /// Creates a command that runs multiple effects sequentially as a single Batch step.
    ///
    /// The Shell executes the effects in order, with no parallelism. This is equivalent
    /// to constructing `CommandStep::Batch` manually.
    pub fn effects(effects: impl IntoIterator<Item = Effect>) -> Self {
        let mut outputs = SmallVec::new();
        let batch: Vec<Effect> = effects.into_iter().collect();
        outputs.push(CommandStep::Batch(batch));
        Self { outputs }
    }

    /// Combines multiple commands into one.
    ///
    /// Flattens all the steps from all commands into a single command.
    /// No magic, no deduplication, no ordering guarantees beyond what
    /// each individual command already provides. If you need specific
    /// ordering, build your commands carefully - this just concatenates.
    ///
    /// # Example
    /// ```
    /// let cmd1 = Command::event(Event::StartLoading);
    /// let cmd2 = Command::effect(Effect::FetchData);
    /// let cmd3 = Command::event(Event::ShowSpinner);
    ///
    /// // Combines all three into one command
    /// let combined = Command::batch(vec![cmd1, cmd2, cmd3]);
    /// ```
    pub fn batch(commands: impl IntoIterator<Item = Self>) -> Self {
        let mut outputs = SmallVec::new();
        for c in commands {
            outputs.extend(c.outputs);
        }
        Self { outputs }
    }

    /// Returns true if this command does nothing.
    ///
    /// Equivalent to checking if `len() == 0`, but doesn't need to
    /// iterate through batch effects to count them. Use this for
    /// quick checks instead of counting steps you don't care about.
    pub fn is_empty(&self) -> bool {
        self.outputs.is_empty()
    }

    /// Returns the total number of steps in this command.
    ///
    /// Counts individual events and effects as 1 each. Batch effects
    /// contribute their length to the total. This walks through all
    /// steps, so it's O(n) where n is the number of `CommandSteps`.
    pub fn len(&self) -> usize {
        self.outputs
            .iter()
            .map(|o| match o {
                CommandStep::Batch(v) => v.len(),
                _ => 1,
            })
            .sum()
    }

    // ---------------------------
    // Builder-style chaining API
    // ---------------------------

    /// Chain another event to this command.
    ///
    /// # Example
    /// ```
    /// let cmd = Command::event(Event::Start)
    ///     .and_event(Event::Initialize)
    ///     .and_event(Event::Ready);
    /// ```
    #[must_use]
    #[inline]
    pub fn and_event(mut self, event: impl Into<Event>) -> Self {
        self.outputs.push(CommandStep::Event(event.into()));
        self
    }

    /// Chain another effect to this command.
    ///
    /// # Example
    /// ```
    /// let cmd = Command::effect(Effect::LoadConfig)
    ///     .and_effect(Effect::ConnectDatabase)
    ///     .and_effect(Effect::StartServer);
    /// ```
    #[must_use]
    #[inline]
    pub fn and_effect(mut self, effect: impl Into<Effect>) -> Self {
        self.outputs.push(CommandStep::Effect(effect.into()));
        self
    }

    /// Chain another command to this one (concatenates all steps).
    ///
    /// # Example
    /// ```
    /// let init = Command::event(Event::Init);
    /// let load = Command::effect(Effect::LoadData);
    /// let combined = init.and(load);
    /// ```
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
    ///
    /// Iterates in the order steps were added. Batch effects appear
    /// as single `CommandStep::Batch` items - this doesn't flatten
    /// them. Use this when you need to inspect or transform the
    /// individual steps in a command.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    enum TestEvent {
        Start,
        Middle,
        End,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum TestEffect {
        Log(String),
        Save,
        Load,
    }

    #[test]
    fn test_chaining_events() {
        let cmd: Command<TestEvent, TestEffect> = Command::event(TestEvent::Start)
            .and_event(TestEvent::Middle)
            .and_event(TestEvent::End);

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], CommandStep::Event(TestEvent::Start));
        assert_eq!(steps[1], CommandStep::Event(TestEvent::Middle));
        assert_eq!(steps[2], CommandStep::Event(TestEvent::End));
    }

    #[test]
    fn test_chaining_effects() {
        let cmd: Command<TestEvent, TestEffect> = Command::effect(TestEffect::Load)
            .and_effect(TestEffect::Save)
            .and_effect(TestEffect::Log("done".into()));

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], CommandStep::Effect(TestEffect::Load));
        assert_eq!(steps[1], CommandStep::Effect(TestEffect::Save));
        assert_eq!(
            steps[2],
            CommandStep::Effect(TestEffect::Log("done".into()))
        );
    }

    #[test]
    fn test_chaining_mixed() {
        let cmd: Command<TestEvent, TestEffect> = Command::event(TestEvent::Start)
            .and_effect(TestEffect::Load)
            .and_event(TestEvent::Middle)
            .and_effect(TestEffect::Save)
            .and_event(TestEvent::End);

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 5);
        assert_eq!(steps[0], CommandStep::Event(TestEvent::Start));
        assert_eq!(steps[1], CommandStep::Effect(TestEffect::Load));
        assert_eq!(steps[2], CommandStep::Event(TestEvent::Middle));
        assert_eq!(steps[3], CommandStep::Effect(TestEffect::Save));
        assert_eq!(steps[4], CommandStep::Event(TestEvent::End));
    }

    #[test]
    fn test_and_command() {
        let cmd1: Command<TestEvent, TestEffect> =
            Command::event(TestEvent::Start).and_effect(TestEffect::Load);

        let cmd2: Command<TestEvent, TestEffect> =
            Command::event(TestEvent::Middle).and_effect(TestEffect::Save);

        let combined = cmd1.and(cmd2);

        let steps: Vec<_> = combined.into_iter().collect();
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0], CommandStep::Event(TestEvent::Start));
        assert_eq!(steps[1], CommandStep::Effect(TestEffect::Load));
        assert_eq!(steps[2], CommandStep::Event(TestEvent::Middle));
        assert_eq!(steps[3], CommandStep::Effect(TestEffect::Save));
    }

    #[test]
    fn test_chaining_from_none() {
        let cmd: Command<TestEvent, TestEffect> = Command::none()
            .and_event(TestEvent::Start)
            .and_effect(TestEffect::Log("hello".into()));

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0], CommandStep::Event(TestEvent::Start));
        assert_eq!(
            steps[1],
            CommandStep::Effect(TestEffect::Log("hello".into()))
        );
    }

    #[test]
    fn test_chaining_preserves_batch() {
        let cmd: Command<TestEvent, TestEffect> =
            Command::effects(vec![TestEffect::Load, TestEffect::Save]).and_event(TestEvent::End);

        let steps: Vec<_> = cmd.into_iter().collect();
        assert_eq!(steps.len(), 2);
        assert_eq!(
            steps[0],
            CommandStep::Batch(vec![TestEffect::Load, TestEffect::Save])
        );
        assert_eq!(steps[1], CommandStep::Event(TestEvent::End));
    }
}
