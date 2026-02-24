pub mod task;

pub use task::Task;

/// Friendly alias for `Task` used in docs to highlight declarative plans.
pub type Plan<E, X> = Task<E, X>;
