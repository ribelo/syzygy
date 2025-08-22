use crate::app::App;
use crate::core::Core;
use crate::shell::Shell;

/// Simple builder for creating Syzygy systems
/// 
/// This builder follows the principle of returning (Core, Shell) tuple
/// for maximum flexibility. Users get direct access to both components
/// and can integrate them however they need.
pub struct SyzygyBuilder<A: App> {
    app: Option<A>,
    model: Option<A::Model>,
    resources: Option<A::Resources>,
}

impl<A: App> SyzygyBuilder<A> {
    /// Create a new builder
    #[must_use] pub fn new() -> Self {
        Self {
            app: None,
            model: None,
            resources: None,
        }
    }
    
    /// Set the application instance
    #[must_use]
    pub fn app(mut self, app: A) -> Self {
        self.app = Some(app);
        self
    }
    
    /// Set the initial model
    #[must_use]
    pub fn model(mut self, model: A::Model) -> Self {
        self.model = Some(model);
        self
    }
    
    /// Set the resources for effect handlers
    /// 
    /// Resources provide typed access to dependencies like database connections,
    /// HTTP clients, file systems, etc. Effect handlers receive these resources
    /// as a parameter, enabling zero-overhead dependency injection.
    #[must_use]
    pub fn resources(mut self, resources: A::Resources) -> Self {
        self.resources = Some(resources);
        self
    }
    
    /// Build the system with auto-wired Shell connected to Core's event channel
    /// 
    /// This is the default and recommended way to build a Syzygy system.
    /// The Shell is automatically connected to Core so effects can send events back.
    /// Both app and model must be provided before calling build().
    pub fn build(self) -> (Core<A>, Shell<A>) {
        let app = self.app.expect("App must be provided before building");
        let model = self.model.expect("Model must be provided before building");
        
        let (core, event_tx) = Core::new(app, model);
        let mut shell = Shell::new().with_event_sender(event_tx);
        
        // Add resources to shell if provided
        if let Some(resources) = self.resources {
            shell = shell.with_resources(resources);
        }
        
        (core, shell)
    }
    
    /// Build the system with manual wiring
    /// 
    /// This gives maximum flexibility by keeping Core and Shell independent.
    /// You must manually connect the Shell to Core's event channel if you want
    /// effects to send events back for processing.
    pub fn build_manual(self) -> (Core<A>, Shell<A>) {
        let app = self.app.expect("App must be provided before building");
        let model = self.model.expect("Model must be provided before building");
        
        let (core, _event_tx) = Core::new(app, model);
        let mut shell = Shell::new();
        
        // Add resources to shell if provided
        if let Some(resources) = self.resources {
            shell = shell.with_resources(resources);
        }
        
        (core, shell)
    }

    // DX helpers removed to keep builder minimal
}

impl<A: App> Default for SyzygyBuilder<A> {
    fn default() -> Self {
        Self::new()
    }
}

/// Main entry point for creating Syzygy systems
pub struct Syzygy;

impl Syzygy {
    /// Create a new builder for the given App type
    #[must_use] pub fn builder<A: App>() -> SyzygyBuilder<A> {
        SyzygyBuilder::new()
    }
    
    /// Create a system with specific app and model (auto-wired by default)
    #[allow(clippy::new_ret_no_self)]
    pub fn new<A: App>(app: A, model: A::Model) -> (Core<A>, Shell<A>) {
        Self::builder().app(app).model(model).build()
    }
    
    /// Create a system with app, model, and resources (auto-wired by default)
    #[allow(clippy::new_ret_no_self)]
    pub fn new_with_resources<A: App>(
        app: A, 
        model: A::Model, 
        resources: A::Resources
    ) -> (Core<A>, Shell<A>) {
        Self::builder().app(app).model(model).resources(resources).build()
    }
    
    /// Create a system with independent Core and Shell (manual wiring)
    pub fn new_manual<A: App>(app: A, model: A::Model) -> (Core<A>, Shell<A>) {
        Self::builder().app(app).model(model).build_manual()
    }
    
    /// Create a system with resources and independent Core and Shell (manual wiring)
    pub fn new_manual_with_resources<A: App>(
        app: A, 
        model: A::Model, 
        resources: A::Resources
    ) -> (Core<A>, Shell<A>) {
        Self::builder().app(app).model(model).resources(resources).build_manual()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::Command;
    
    #[derive(Debug, Default)]
    struct TestApp;
    
    #[derive(Debug, Clone)]
    enum TestEvent {
        Increment,
    }
    
    #[derive(Debug, Default)]
    struct TestModel {
        count: i32,
    }
    
    #[derive(Debug, Clone)]
    enum TestEffect {
        Log,
    }
    
    #[derive(Debug, Clone)]
    struct TestResources;
    
    impl App for TestApp {
        type Event = TestEvent;
        type Model = TestModel;
        #[cfg(feature = "view-model")]
        type ViewModel = i32;
        type Effect = TestEffect;
        type Resources = TestResources;
        
        fn update(&self, event: Self::Event, model: &mut Self::Model) -> Command<Self::Event, Self::Effect> {
            match event {
                TestEvent::Increment => {
                    model.count += 1;
                    Command::effect(TestEffect::Log)
                }
            }
        }
        
        #[cfg(feature = "view-model")]
        fn view(&self, model: &Self::Model) -> Self::ViewModel {
            model.count
        }
    }
    
    #[test]
    fn test_builder() {
        let (mut core, _shell) = Syzygy::builder::<TestApp>()
            .app(TestApp)
            .model(TestModel { count: 0 })
            .build();
        
        // Test that we can handle events through the core
        let _command = core.handle_event(TestEvent::Increment);
        assert_eq!(core.model().count, 1);
        
        // Command should contain an effect
        // (We can't easily test this without executing the command stream)
    }
    
    #[test]
    fn test_convenience_methods() {
        let (_core, _shell) = Syzygy::new(TestApp, TestModel { count: 0 });
    }
    
    #[test]
    fn test_auto_wiring() {
        let (core, _shell) = Syzygy::builder::<TestApp>()
            .app(TestApp)
            .model(TestModel { count: 0 })
            .build(); // Default is now auto-wired
        
        // Verify that Shell has been wired to Core's event channel
        let _event_sender = core.event_sender();
        
        // The shell should be able to receive events sent through this channel
        // (This is hard to test without actually running the async machinery,
        //  but we can at least verify the setup doesn't panic)
        
        // Test convenience method too (default is auto-wired)
        let (_core2, _shell2) = Syzygy::new(TestApp, TestModel { count: 0 });
    }
    
    #[test]
    fn test_manual_wiring() {
        let (core, _shell) = Syzygy::builder::<TestApp>()
            .app(TestApp)
            .model(TestModel { count: 0 })
            .build_manual(); // Explicit manual wiring
        
        let _event_sender = core.event_sender();
        
        // Test convenience method for manual wiring
        let (_core2, _shell2) = Syzygy::new_manual(TestApp, TestModel { count: 0 });
    }
}
