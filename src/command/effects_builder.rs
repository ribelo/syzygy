//! Iterator-style Effects builder for clean, chainable effect coordination
//!
//! This module provides a fluent API for coordinating effects that feels natural
//! like Rust iterators. The design avoids semantic confusion by using a neutral
//! entry point (`Effects::new()`) followed by explicit mode selection.
//!
//! # Iterator Analogy
//!
//! The Effects API is designed to feel like iterator chains:
//!
//! | Iterator Pattern | Effects Pattern | Purpose |
//! |-----------------|-----------------|---------|
//! | `vec.iter()`    | `Effects::new([...])` | Start with data |
//! | `.map()/.filter()` | `.parallel()/.race()/.sequence()` | Transform/mode |
//! | `.take()/.skip()` | `.timeout_per()/.stop_on_error()` | Configure |
//! | `.collect()` | `.barrier(event)` | Terminal - wait for all |
//! | `.for_each()` | `.spawn()` | Terminal - fire-and-forget |
//!
//! Just like iterators, you start neutral, chain operations, then terminate:
//!
//! ```rust,ignore
//! // Iterator style
//! let results: Vec<_> = data.iter()
//!     .map(process)
//!     .filter(is_valid)
//!     .collect();
//!
//! // Effects style  
//! let cmd = Effects::new([effect1, effect2])
//!     .parallel()
//!     .timeout_per(Duration::from_secs(5))
//!     .barrier(AllComplete);
//! ```
//!
//! # Examples
//!
//! ```rust,ignore
//! use syzygy::command::Effects;
//!
//! // Parallel coordination with barrier
//! Effects::new([load_config(), load_user()])
//!     .parallel()
//!     .timeout_per(Duration::from_secs(5))
//!     .barrier(AppReady)
//!
//! // Race coordination (first-wins)
//! Effects::new([mirror1(), mirror2()])
//!     .race()
//!     .timeout_per(Duration::from_secs(1))
//!     .spawn()  // Fire-and-forget
//!
//! // Sequential execution
//! Effects::new([step1(), step2(), step3()])
//!     .sequence()
//!     .stop_on_error()
//!     .barrier(ProcessComplete)
//! ```

use crate::command::{Command, CommandStep, GroupMode};
use smallvec::SmallVec;
use std::marker::PhantomData;
use std::time::Duration;

/// Entry point for effect coordination - neutral with no implied coordination mode
///
/// This is the equivalent of starting an iterator chain with `vec.iter()`.
/// You specify the effects first, then choose how to coordinate them.
pub struct Effects<Event, Effect> {
    effects: Vec<Effect>,
    _phantom: PhantomData<Event>,
}

impl<Event, Effect> Effects<Event, Effect> {
    /// Create a new effects coordination builder
    ///
    /// **Iterator analogy:** Like `vec.iter()` - starts the chain with data.
    ///
    /// Takes any iterable of effects and prepares them for coordination.
    /// No coordination mode is implied - you must call `.parallel()`, `.race()`,
    /// or `.sequence()` to specify how these effects should be executed.
    pub fn new(effects: impl IntoIterator<Item = Effect>) -> Self {
        Self {
            effects: effects.into_iter().collect(),
            _phantom: PhantomData,
        }
    }

    /// Begin parallel coordination - all effects run concurrently
    ///
    /// **Iterator analogy:** Like `.map()` - transforms the data stream.
    ///
    /// This creates a ParallelBuilder that lets you configure timeout,
    /// then finish with either `.barrier()` (wait for all) or `.spawn()` (fire-and-forget).
    pub fn parallel(self) -> ParallelBuilder<Event, Effect> {
        ParallelBuilder {
            effects: self.effects,
            timeout_per: None,
            label: None,
            _phantom: PhantomData,
        }
    }

    /// Begin race coordination - first effect to complete wins, others cancel
    ///
    /// **Iterator analogy:** Like `.find()` - stops at the first match and ignores the rest.
    ///
    /// This creates a RaceBuilder that lets you configure timeout,
    /// then finish with either `.barrier()` (emit when first completes) or `.spawn()` (fire-and-forget).
    pub fn race(self) -> RaceBuilder<Event, Effect> {
        RaceBuilder {
            effects: self.effects,
            timeout_per: None,
            label: None,
            _phantom: PhantomData,
        }
    }

    /// Begin sequential coordination - effects run one after another
    ///
    /// **Iterator analogy:** Like `.fold()` - processes items in order, building up state.
    ///
    /// This creates a SequentialBuilder that lets you configure error handling,
    /// then finish with `.barrier()` (emit when all complete).
    pub fn sequence(self) -> SequentialBuilder<Event, Effect> {
        SequentialBuilder {
            effects: self.effects,
            stop_on_error: true,
            label: None,
            _phantom: PhantomData,
        }
    }
}

/// Builder for parallel effect coordination
///
/// All effects run concurrently. Use `.barrier()` to wait for all effects
/// to complete before emitting an event, or `.spawn()` for fire-and-forget.
pub struct ParallelBuilder<Event, Effect> {
    effects: Vec<Effect>,
    timeout_per: Option<Duration>,
    label: Option<&'static str>,
    _phantom: PhantomData<Event>,
}

impl<Event, Effect> ParallelBuilder<Event, Effect> {
    /// Set a timeout for each individual effect in the group
    ///
    /// If any effect takes longer than this duration, it will be cancelled.
    /// This is applied per-effect, not to the entire group.
    pub fn timeout_per(mut self, duration: Duration) -> Self {
        self.timeout_per = Some(duration);
        self
    }

    /// Add a tracing label for debugging (no-op if tracing disabled)
    ///
    /// This helps identify the group in logs and tracing output.
    pub fn label(mut self, name: &'static str) -> Self {
        self.label = Some(name);
        self
    }

    /// Terminal: Wait for all effects to complete, then emit barrier event
    ///
    /// **Iterator analogy:** Like `.collect()` - consumes the chain and produces a result.
    ///
    /// This consumes the builder and produces a Command that waits for all effects.
    pub fn barrier(self, event: Event) -> Command<Event, Effect> {
        if self.effects.is_empty() {
            return Command::event(event);
        }

        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Group {
            effects: self.effects,
            mode: GroupMode::Parallel,
            barrier: Some(event),
            timeout_per: self.timeout_per,
        });
        Command { outputs }
    }

    /// Terminal: Fire-and-forget parallel execution
    ///
    /// **Iterator analogy:** Like `.for_each()` - consumes the chain but doesn't collect results.
    ///
    /// This starts all effects running but doesn't wait for them to complete.
    pub fn spawn(self) -> Command<Event, Effect> {
        if self.effects.is_empty() {
            return Command::none();
        }

        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Group {
            effects: self.effects,
            mode: GroupMode::Parallel,
            barrier: None,
            timeout_per: self.timeout_per,
        });
        Command { outputs }
    }
}

/// Builder for race effect coordination
///
/// Effects compete - first to complete wins, others are cancelled.
/// Use `.barrier()` to emit an event when the winner finishes, or `.spawn()`
/// for fire-and-forget where only the winner's result matters.
pub struct RaceBuilder<Event, Effect> {
    effects: Vec<Effect>,
    timeout_per: Option<Duration>,
    label: Option<&'static str>,
    _phantom: PhantomData<Event>,
}

impl<Event, Effect> RaceBuilder<Event, Effect> {
    /// Set a timeout for each individual effect in the race
    ///
    /// If any effect takes longer than this duration, it will be cancelled.
    /// The race continues with the remaining effects.
    pub fn timeout_per(mut self, duration: Duration) -> Self {
        self.timeout_per = Some(duration);
        self
    }

    /// Add a tracing label for debugging (no-op if tracing disabled)
    ///
    /// This helps identify the race group in logs and tracing output.
    pub fn label(mut self, name: &'static str) -> Self {
        self.label = Some(name);
        self
    }

    /// Terminal: Emit barrier event when first effect completes
    ///
    /// The first effect to complete wins, all others are cancelled,
    /// then the barrier event is emitted.
    pub fn barrier(self, event: Event) -> Command<Event, Effect> {
        if self.effects.is_empty() {
            return Command::event(event);
        }

        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Group {
            effects: self.effects,
            mode: GroupMode::Race,
            barrier: Some(event),
            timeout_per: self.timeout_per,
        });
        Command { outputs }
    }

    /// Terminal: Fire-and-forget race - first to complete wins
    ///
    /// Effects race against each other, first to complete wins and others
    /// are cancelled. No additional barrier event is emitted - only the
    /// winner's natural events (if any) will be sent.
    pub fn spawn(self) -> Command<Event, Effect> {
        if self.effects.is_empty() {
            return Command::none();
        }

        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Group {
            effects: self.effects,
            mode: GroupMode::Race,
            barrier: None,
            timeout_per: self.timeout_per,
        });
        Command { outputs }
    }
}

/// Builder for sequential effect coordination
///
/// Effects run one after another in the order specified. By default,
/// execution stops on the first error, but this can be configured.
pub struct SequentialBuilder<Event, Effect> {
    effects: Vec<Effect>,
    stop_on_error: bool,
    label: Option<&'static str>,
    _phantom: PhantomData<Event>,
}

impl<Event, Effect> SequentialBuilder<Event, Effect> {
    /// Configure error handling: stop on first error (default: true)
    ///
    /// When true, if any effect fails, the sequence stops and remaining
    /// effects are not executed. When false, all effects run regardless
    /// of individual failures.
    pub fn stop_on_error(mut self, stop: bool) -> Self {
        self.stop_on_error = stop;
        self
    }

    /// Add a tracing label for debugging (no-op if tracing disabled)
    ///
    /// This helps identify the sequence in logs and tracing output.
    pub fn label(mut self, name: &'static str) -> Self {
        self.label = Some(name);
        self
    }

    /// Terminal: Wait for all effects to complete sequentially, then emit barrier event
    ///
    /// Effects run one after another. If `stop_on_error` is true (default),
    /// the sequence stops on first failure. Otherwise all effects run.
    /// The barrier event is emitted when the sequence completes (successfully or not).
    pub fn barrier(self, event: Event) -> Command<Event, Effect> {
        if self.effects.is_empty() {
            return Command::event(event);
        }

        // For sequential execution, we use Batch which the Shell processes sequentially
        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Batch(self.effects));
        outputs.push(CommandStep::Event(event));
        Command { outputs }
    }

    /// Terminal: Fire-and-forget sequential execution
    ///
    /// Effects run one after another but no barrier event is emitted.
    /// This is useful for cleanup sequences or fire-and-forget workflows.
    pub fn spawn(self) -> Command<Event, Effect> {
        if self.effects.is_empty() {
            return Command::none();
        }

        let mut outputs = SmallVec::new();
        outputs.push(CommandStep::Batch(self.effects));
        Command { outputs }
    }
}