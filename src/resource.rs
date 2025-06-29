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
}

pub trait ResourceAccess: Context {
    fn resources(&self) -> &Resources;
    fn resource<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.resources().get::<T>().unwrap()
    }
    fn resource_cloned<T>(&self) -> T
    where
        T: Clone + Send + Sync + 'static,
    {
        self.resources().get_cloned::<T>().unwrap()
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
}

pub trait ResourceModify: ResourceAccess {
    fn add_resource<T>(&self, value: T)
    where
        T: Send + Sync + 'static,
    {
        let arc_value = Arc::new(value);
        self.resources()
            .write()
            .expect("Failed to acquire write lock")
            .insert(TypeId::of::<T>(), Box::new(arc_value));
    }

    fn remove_resource<T>(&self) -> Option<Box<dyn Any + Send + Sync>>
    where
        T: Send + Sync + 'static,
    {
        self.resources()
            .write()
            .expect("Failed to acquire write lock")
            .remove(&TypeId::of::<T>())
    }
}
