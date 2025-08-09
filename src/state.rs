//! Unified state access traits that combine model and resource operations
//!
//! This module provides consolidated traits that combine model and resource
//! access patterns for a simpler API surface.

use crate::{
    context::Context,
    model::{ModelAccess, ModelModify},
    resource::{ResourceAccess, ResourceModify},
};

/// Unified read-only access to both model and resources
pub trait StateAccess: Context + ModelAccess + ResourceAccess {
    /// Query both model and resources in a single operation
    fn query_state<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Self::Model, &crate::resource::Resources) -> R,
    {
        f(self.model(), self.resources())
    }

    /// Access model and a specific resource together
    fn with_model_and_resource<T, F, R>(&self, f: F) -> R
    where
        T: Send + Sync + std::fmt::Debug + 'static,
        F: FnOnce(&Self::Model, &T) -> R,
    {
        let resource = self.resource::<T>();
        f(self.model(), &*resource)
    }
}

/// Unified read-write access to both model and resources
pub trait StateModify: StateAccess + ModelModify + ResourceModify {
    /// Replace a resource and update the model based on the old value
    fn replace_resource_and_update<T, F, R>(&mut self, new_resource: T, f: F) -> R
    where
        T: Send + Sync + std::fmt::Debug + 'static,
        F: FnOnce(&mut Self::Model, Option<std::sync::Arc<T>>) -> R,
    {
        let old_resource = self.set_resource(new_resource);
        f(self.model_mut(), old_resource)
    }

    /// Update model, then access a resource
    fn update_model_then_access_resource<T, F, G, R>(&mut self, update_f: F, access_f: G) -> R
    where
        T: Send + Sync + std::fmt::Debug + 'static,
        F: FnOnce(&mut Self::Model),
        G: FnOnce(&T) -> R,
    {
        update_f(self.model_mut());
        let resource = self.resource::<T>();
        access_f(&*resource)
    }
}

// Blanket implementations for types that already implement the component traits
impl<T> StateAccess for T where T: Context + ModelAccess + ResourceAccess {}
impl<T> StateModify for T where
    T: Context + ModelAccess + ResourceAccess + ModelModify + ResourceModify
{
}
