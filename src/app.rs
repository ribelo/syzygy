use crate::command::Command;

/// The core trait that users implement to define their application logic.
/// 
/// This trait follows The Elm Architecture pattern, providing a clean interface
/// for event-driven applications with pure functional updates.
pub trait App: Send + 'static {
    /// The event type - actions that trigger state changes
    type Event: Clone + Send + 'static;
    
    /// The model type - internal application state
    type Model: Send + 'static;
    
    /// The view model type - UI representation of state
    /// Only available when the "view-model" feature is enabled
    #[cfg(feature = "view-model")]
    type ViewModel: Send + 'static;
    
    /// The effect type - descriptions of side effects to execute
    type Effect: Clone + Send + 'static;
    
    /// Resources/dependencies available to effect handlers
    /// 
    /// Resources provide dependencies like database connections, HTTP clients,
    /// file systems, etc. to effect handlers through compile-time type safety.
    /// This enables zero-overhead dependency injection with explicit dependencies.
    /// 
    /// Example:
    /// ```rust,ignore
    /// #[derive(Clone)]
    /// struct MyResources {
    ///     database: Arc<dyn Database>,
    ///     http: Arc<dyn HttpClient>,
    /// }
    /// 
    /// impl App for MyApp {
    ///     type Resources = MyResources;
    ///     // ... other types
    /// }
    /// ```
    type Resources: Send + Sync + 'static;
    
    /// Process an event and update the model, returning a command that
    /// describes any effects to execute or follow-up events to process.
    /// 
    /// This function should be pure - no I/O, no side effects, just
    /// model updates and command creation.
    fn update(
        &self, 
        event: Self::Event, 
        model: &mut Self::Model
    ) -> Command<Self::Event, Self::Effect>;
    
    /// Transform the model into a view model for UI rendering.
    /// 
    /// Only available when the "view-model" feature is enabled.
    #[cfg(feature = "view-model")]
    fn view(&self, model: &Self::Model) -> Self::ViewModel;
}