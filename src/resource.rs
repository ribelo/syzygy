use std::{
    any::{Any, TypeId},
    ops::Deref,
    sync::{Arc, RwLock},
};

use rustc_hash::FxHashMap;

use crate::context::Context;

#[derive(Default, Debug, Clone)]
pub struct Resources(Arc<RwLock<FxHashMap<TypeId, Box<dyn Any + Send + Sync>>>>);

impl Deref for Resources {
    type Target = RwLock<FxHashMap<TypeId, Box<dyn Any + Send + Sync>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Resources {
    pub fn insert<T>(&mut self, value: T)
    where
        T: Send + Sync + 'static,
    {
        let ty = TypeId::of::<T>();
        let arc_value = Arc::new(value);
        let boxed_value = Box::new(arc_value);
        let mut lock = self.write().expect("Failed to acquire write lock");
        lock.insert(ty, boxed_value);
    }

    #[must_use]
    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let ty = TypeId::of::<T>();
        let lock = self.read().expect("Failed to acquire read lock");
        lock.get(&ty)
            .and_then(|boxed_value| boxed_value.downcast_ref::<Arc<T>>().cloned())
    }

    #[must_use]
    pub fn get_cloned<T>(&self) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.get::<T>().map(|arc| (*arc).clone())
    }

    /// Update a resource in-place with a function
    /// Note: This requires the resource to have interior mutability (e.g., Arc<Mutex<T>>)
    pub fn update_resource<T, F, R>(&self, f: F) -> Option<R>
    where
        T: Send + Sync + 'static,
        F: FnOnce(&T) -> R,
    {
        self.get::<T>().map(|resource| f(&*resource))
    }

    /// Replace a resource entirely and return the old version
    pub fn replace_resource<T>(&self, new_value: T) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let ty = TypeId::of::<T>();
        let arc_value = Arc::new(new_value);
        let boxed_value = Box::new(arc_value.clone());
        
        let mut lock = self.write().expect("Failed to acquire write lock");
        let old_boxed = lock.insert(ty, boxed_value);
        
        old_boxed.and_then(|old| old.downcast_ref::<Arc<T>>().cloned())
    }
}

pub trait ResourceAccess: Context {
    fn resources(&self) -> &Resources;
    fn resource<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.resources().get::<T>()
            .unwrap_or_else(|| panic!("Resource of type {} not found", std::any::type_name::<T>()))
    }
    fn resource_cloned<T>(&self) -> T
    where
        T: Clone + Send + Sync + 'static,
    {
        self.resources().get_cloned::<T>()
            .unwrap_or_else(|| panic!("Resource of type {} not found", std::any::type_name::<T>()))
    }
    fn try_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.resources().get::<T>()
    }
    fn try_resource_cloned<T>(&self) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.resources().get_cloned::<T>()
    }
    fn with_resource<T, F, R>(&self, f: F) -> R
    where
        T: Send + Sync + 'static,
        F: FnOnce(&T) -> R,
    {
        let arc = self.resource::<T>();
        f(&*arc)
    }

    /// Get a resource or panic with a descriptive error message
    fn expect_resource<T>(&self, msg: &str) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.try_resource::<T>()
            .unwrap_or_else(|| panic!("Resource of type {} not found: {}", std::any::type_name::<T>(), msg))
    }

    /// Get a cloned resource or panic with a descriptive error message
    fn expect_resource_cloned<T>(&self, msg: &str) -> T
    where
        T: Clone + Send + Sync + 'static,
    {
        self.try_resource_cloned::<T>()
            .unwrap_or_else(|| panic!("Resource of type {} not found: {}", std::any::type_name::<T>(), msg))
    }
}

pub trait ResourceModify: ResourceAccess {
    fn add_resource<T>(&self, value: T)
    where
        T: Send + Sync + 'static,
    {
        let arc_value = Arc::new(value);
        let boxed_value = Box::new(arc_value);
        self.resources()
            .write()
            .expect("Failed to acquire write lock")
            .insert(TypeId::of::<T>(), boxed_value);
    }

    fn remove_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let removed = self.resources()
            .write()
            .expect("Failed to acquire write lock")
            .remove(&TypeId::of::<T>());
        
        removed.and_then(|boxed| boxed.downcast_ref::<Arc<T>>().cloned())
    }

    /// Update a resource in-place using a closure
    /// Note: This requires the resource to have interior mutability (e.g., Arc<Mutex<T>>)
    fn update_resource<T, F, R>(&self, f: F) -> Option<R>
    where
        T: Send + Sync + 'static,
        F: FnOnce(&T) -> R,
    {
        self.resources().update_resource(f)
    }

    /// Replace a resource entirely and return the old version
    fn replace_resource<T>(&self, new_value: T) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.resources().replace_resource(new_value)
    }

    /// Get mutable access to the entire resources map for batch operations
    /// This is expensive as it locks the entire resource map
    fn with_resources_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut FxHashMap<TypeId, Box<dyn Any + Send + Sync>>) -> R,
    {
        let mut lock = self.resources().write().expect("Failed to acquire write lock");
        f(&mut *lock)
    }
}
