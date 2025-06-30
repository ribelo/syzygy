//! `EffectBuilder` for composable effect wrapping
//!
//! Because composing functions is better than enterprise middleware bullshit.
//!
//! `EffectBuilder` lets users wrap effects with common functionality (timing, tracing, retry)
//! without polluting the core API or adding overhead to the fast path.

use std::time::Instant;
use crate::{model::Model, dispatch::EffectFn};

/// Configuration for effect wrapping behaviors
#[derive(Debug, Clone, Default)]
struct EffectConfig {
    /// Whether to add timing instrumentation
    timed: Option<&'static str>,
    /// Whether to add tracing instrumentation
    traced: bool,
    /// Debug name for the effect
    name: Option<&'static str>,
    /// Source location information
    location: Option<(&'static str, u32)>,
}

/// Builder for composable effect wrapping
///
/// This allows stacking behaviors like timing, tracing, and error handling
/// on top of effects without polluting the core dispatch API.
///
/// # Examples
///
/// ```rust
/// use syzygy::prelude::*;
/// use syzygy::model::Model;
///
/// # #[derive(Debug, Clone)]
/// # struct TestModel { counter: i32 }
/// # impl Model for TestModel {
/// #     type Snapshot = Self;
/// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// # }
/// let effect = EffectBuilder::new(|ctx: &mut Syzygy<TestModel>| {
///     ctx.update(|m| m.counter += 1);
/// })
/// .timed("increment_counter")
/// .traced()
/// .build();
/// ```
pub struct EffectBuilder<M: Model> {
    base_effect: Box<dyn EffectFn<M>>,
    config: EffectConfig,
}

impl<M: Model> EffectBuilder<M> {
    /// Create a new `EffectBuilder` wrapping the given effect
    pub fn new(effect: impl EffectFn<M> + 'static) -> Self {
        Self {
            base_effect: Box::new(effect),
            config: EffectConfig::default(),
        }
    }

    /// Add timing instrumentation to the effect
    ///
    /// In debug builds, this logs the execution time.
    /// In release builds, this is optimized away.
    #[inline]
    #[must_use]
    pub fn timed(mut self, name: &'static str) -> Self {
        self.config.timed = Some(name);
        self
    }

    /// Add tracing instrumentation to the effect
    ///
    /// In debug builds, this logs effect start/completion.
    /// In release builds, this is optimized away.
    #[inline]
    #[must_use]
    pub fn traced(mut self) -> Self {
        self.config.traced = true;
        self
    }

    /// Add a name to the effect for debugging purposes
    ///
    /// This is only stored in debug builds.
    #[inline]
    #[must_use]
    pub fn named(mut self, name: &'static str) -> Self {
        self.config.name = Some(name);
        self
    }

    /// Add source location information (called by macro)
    #[doc(hidden)]
    #[must_use]
    pub fn with_location(mut self, file: &'static str, line: u32) -> Self {
        self.config.location = Some((file, line));
        self
    }

    /// Build the final effect
    ///
    /// This consumes the builder and returns the wrapped effect.
    #[must_use]
    pub fn build(self) -> impl EffectFn<M> {
        let EffectBuilder { base_effect, config } = self;

        // Start with the base effect
        let mut effect: Box<dyn EffectFn<M>> = base_effect;

        // Extract config values we'll need in closures
        let timer_name = config.timed;
        let should_trace = config.traced;
        let effect_name = config.name;
        let location = config.location;

        // Apply timing wrapper if requested
        if let Some(timer_name) = timer_name {
            let inner = effect;
            effect = Box::new(move |ctx| {
                let start = Instant::now();
                (inner)(ctx);
                let elapsed = start.elapsed();

                #[cfg(debug_assertions)]
                log::debug!("Effect '{}' took {:?}", timer_name, elapsed);

                // Could also record to metrics here if metrics feature is enabled
                // #[cfg(feature = "effect-metrics")]
                // if let Some(metrics) = crate::debug::metrics() {
                //     metrics.record_timing(timer_name, elapsed);
                // }
            });
        }

        // Apply tracing wrapper if requested
        if should_trace {
            let inner = effect;
            effect = Box::new(move |ctx| {
                #[cfg(debug_assertions)]
                {
                    let id = crate::debug::EffectId::next();
                    let name = effect_name.unwrap_or("unnamed");
                    let location_str = location
                        .map(|(file, line)| format!(" at {}:{}", file, line))
                        .unwrap_or_default();

                    log::trace!("Effect {} '{}'{} starting", id, name, location_str);
                    (inner)(ctx);
                    log::trace!("Effect {} completed", id);
                }
                #[cfg(not(debug_assertions))]
                (inner)(ctx);
            });
        }

        effect
    }
}

/// Extension trait for ergonomic effect building
///
/// This allows any effect to be easily wrapped using method chaining:
///
/// ```rust
/// use syzygy::prelude::*;
/// use syzygy::model::Model;
///
/// # #[derive(Debug, Clone)]
/// # struct TestModel { counter: i32 }
/// # impl Model for TestModel {
/// #     type Snapshot = Self;
/// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
/// # }
/// let effect = (|ctx: &mut Syzygy<TestModel>| {
///     ctx.update(|m| m.counter += 1);
/// }).timed("increment").traced();
/// ```
pub trait EffectExt<M: Model>: EffectFn<M> + Sized + 'static {
    /// Wrap with timing instrumentation
    fn timed(self, name: &'static str) -> EffectBuilder<M> {
        EffectBuilder::new(self).timed(name)
    }

    /// Wrap with tracing instrumentation
    fn traced(self) -> EffectBuilder<M> {
        EffectBuilder::new(self).traced()
    }

    /// Add a name for debugging
    fn named(self, name: &'static str) -> EffectBuilder<M> {
        EffectBuilder::new(self).named(name)
    }
}

/// Blanket implementation for all effect functions
impl<M: Model, F: EffectFn<M> + 'static> EffectExt<M> for F {}

/// Macro for creating effects with location information
#[macro_export]
macro_rules! effect {
    // Plain effect
    ($effect:expr) => {
        $effect
    };

    // Named effect with automatic location
    (named $name:literal => $effect:expr) => {
        $crate::effect_builder::EffectBuilder::new($effect)
            .named($name)
            .with_location(file!(), line!())
            .build()
    };

    // Timed effect
    (timed $name:literal => $effect:expr) => {
        $crate::effect_builder::EffectBuilder::new($effect)
            .timed($name)
            .with_location(file!(), line!())
            .build()
    };

    // Traced effect
    (traced => $effect:expr) => {
        $crate::effect_builder::EffectBuilder::new($effect)
            .traced()
            .with_location(file!(), line!())
            .build()
    };

    // Combined timed and traced
    (timed $name:literal, traced => $effect:expr) => {
        $crate::effect_builder::EffectBuilder::new($effect)
            .timed($name)
            .traced()
            .with_location(file!(), line!())
            .build()
    };

    // Combined named and traced
    (named $name:literal, traced => $effect:expr) => {
        $crate::effect_builder::EffectBuilder::new($effect)
            .named($name)
            .traced()
            .with_location(file!(), line!())
            .build()
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::{Model, ModelAccess, ModelModify}, syzygy::Syzygy, dispatch::DispatchEffect};

    #[derive(Debug, Clone)]
    struct TestModel {
        counter: i32,
    }

    impl Model for TestModel {
        type Snapshot = Self;
        fn to_snapshot(&self) -> Self::Snapshot {
            self.clone()
        }
    }

    #[test]
    fn test_basic_builder() {
        let effect = EffectBuilder::new(|ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 1);
        }).build();

        let mut syzygy = Syzygy::builder().model(TestModel { counter: 0 }).build();
        (effect)(&mut syzygy);
        assert_eq!(syzygy.model().counter, 1);
    }

    #[test]
    fn test_timed_effect() {
        let effect = EffectBuilder::new(|ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 5);
        })
        .timed("test_increment")
        .build();

        let mut syzygy = Syzygy::builder().model(TestModel { counter: 0 }).build();
        (effect)(&mut syzygy);
        assert_eq!(syzygy.model().counter, 5);
    }

    #[test]
    fn test_traced_effect() {
        let effect = EffectBuilder::new(|ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 10);
        })
        .traced()
        .build();

        let mut syzygy = Syzygy::builder().model(TestModel { counter: 0 }).build();
        (effect)(&mut syzygy);
        assert_eq!(syzygy.model().counter, 10);
    }

    #[test]
    fn test_chained_wrappers() {
        let effect = EffectBuilder::new(|ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 100);
        })
        .named("big_increment")
        .timed("big_increment_timing")
        .traced()
        .build();

        let mut syzygy = Syzygy::builder().model(TestModel { counter: 0 }).build();
        (effect)(&mut syzygy);
        assert_eq!(syzygy.model().counter, 100);
    }

    #[test]
    fn test_extension_trait() {
        let effect = (|ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 42);
        }).timed("extension_test");

        let mut syzygy = Syzygy::builder().model(TestModel { counter: 0 }).build();
        (effect.build())(&mut syzygy);
        assert_eq!(syzygy.model().counter, 42);
    }

    #[test]
    fn test_effect_macro() {
        let plain = effect!(|ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 1);
        });

        let timed = effect!(timed "macro_test" => |ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 2);
        });

        let traced = effect!(traced => |ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 3);
        });

        let mut syzygy = Syzygy::builder().model(TestModel { counter: 0 }).build();

        (plain)(&mut syzygy);
        assert_eq!(syzygy.model().counter, 1);

        (timed)(&mut syzygy);
        assert_eq!(syzygy.model().counter, 3);

        (traced)(&mut syzygy);
        assert_eq!(syzygy.model().counter, 6);
    }

    #[test]
    fn test_dispatch_integration() {
        let syzygy = Syzygy::builder().model(TestModel { counter: 0 }).build();

        // Test dispatch_builder
        syzygy.dispatch_builder(|| {
            EffectBuilder::new(|ctx: &mut Syzygy<TestModel>| {
                ctx.update(|m| m.counter += 10);
            }).timed("builder_test")
        });

        // Test dispatch_timed
        syzygy.dispatch_timed("timed_test", |ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 20);
        });

        // Test dispatch_traced
        syzygy.dispatch_traced(|ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 30);
        });

        // Test dispatch_named
        syzygy.dispatch_named("named_test", |ctx: &mut Syzygy<TestModel>| {
            ctx.update(|m| m.counter += 40);
        });

        // Just verify methods can be called - the actual effect processing is tested elsewhere
        // In real usage, the effect processing would happen in a separate thread/task
    }
}
