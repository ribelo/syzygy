use std::{
    any::{Any, TypeId},
    ops::Deref,
    sync::{Arc, RwLock},
};

use rustc_hash::FxHashMap;

use crate::{context::Context, error::ResourceNotFoundError};

/// Trait for cloneable type-erased resources
pub trait CloneableAny: Any + Send + Sync + std::fmt::Debug {
    fn clone_box(&self) -> Box<dyn CloneableAny>;
    fn as_any(&self) -> &dyn Any;
}

impl<T: Clone + Send + Sync + std::fmt::Debug + 'static> CloneableAny for T {
    fn clone_box(&self) -> Box<dyn CloneableAny> {
        Box::new(self.clone())
    }
    
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl Clone for Box<dyn CloneableAny> {
    fn clone(&self) -> Box<dyn CloneableAny> {
        self.clone_box()
    }
}

/// Copy-on-write resource storage with lock-free reads
///
/// This structure uses a double-Arc pattern for superior performance:
/// - Outer Arc allows cheap cloning of the Resources struct
/// - RwLock protects access to the inner Arc pointer  
/// - Inner Arc<HashMap> enables lock-free reads after acquiring the map reference
///
/// Read operations:
/// 1. Acquire read lock (fast, shared)
/// 2. Clone inner Arc<HashMap> (cheap atomic ref increment)
/// 3. Release read lock immediately
/// 4. Perform HashMap lookup with zero contention
///
/// Write operations:
/// 1. Acquire write lock (blocks other writes only)
/// 2. Arc::make_mut() ensures unique HashMap access (COW)
/// 3. Modify HashMap and release write lock
#[derive(Debug, Clone)]
pub struct Resources(Arc<RwLock<Arc<FxHashMap<TypeId, Box<dyn CloneableAny>>>>>);

impl Default for Resources {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(Arc::new(FxHashMap::default()))))
    }
}

impl Deref for Resources {
    type Target = RwLock<Arc<FxHashMap<TypeId, Box<dyn CloneableAny>>>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Resources {
    /// Insert a resource using copy-on-write semantics
    /// 
    /// This operation:
    /// 1. Acquires a write lock on the outer RwLock
    /// 2. Uses Arc::make_mut to get unique access to the HashMap (COW)
    /// 3. Inserts the resource and releases the lock
    pub fn insert<T>(&self, value: T)
    where
        T: Send + Sync + std::fmt::Debug + 'static,
    {
        let ty = TypeId::of::<T>();
        let arc_value = Arc::new(value);
        let boxed_value = Box::new(arc_value);
        
        let mut write_guard = self.write().expect("Failed to acquire write lock");
        let map = Arc::make_mut(&mut write_guard);
        map.insert(ty, boxed_value);
    }

    /// Get a resource using lock-free read semantics
    /// 
    /// This operation:
    /// 1. Acquires a read lock (fast, shared with other readers)
    /// 2. Clones the inner Arc<HashMap> (cheap atomic ref increment)
    /// 3. Releases the read lock immediately  
    /// 4. Performs HashMap lookup with zero contention
    #[must_use]
    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
    {
        let ty = TypeId::of::<T>();
        
        // Step 1 & 2: Acquire read lock and clone the map Arc (lock held briefly)
        let map = {
            let read_guard = self.read().expect("Failed to acquire read lock");
            Arc::clone(&read_guard)
        };
        // Step 3: Read lock is now released
        
        // Step 4: Perform lookup with zero contention
        map.get(&ty)
            .and_then(|boxed_value| boxed_value.as_any().downcast_ref::<Arc<T>>())
            .cloned()
    }


}

pub trait ResourceAccess: Context {
    fn resources(&self) -> &Resources;
    fn resource<T>(&self) -> Arc<T>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
    {
        self.resources()
            .get::<T>()
            .unwrap_or_else(|| panic!("Resource of type {} not found", std::any::type_name::<T>()))
    }
    fn try_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
    {
        self.resources().get::<T>()
    }
    fn with_resource<T, F, R>(&self, f: F) -> R
    where
        T: Send + Sync + std::fmt::Debug + 'static,
        F: FnOnce(&T) -> R,
    {
        let arc = self.resource::<T>();
        f(&*arc)
    }

    /// Get a resource or panic with a descriptive error message
    fn expect_resource<T>(&self, msg: &str) -> Arc<T>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
    {
        self.try_resource::<T>().unwrap_or_else(|| {
            panic!(
                "Resource of type {} not found: {}",
                std::any::type_name::<T>(),
                msg
            )
        })
    }


    /// Get a resource or return a ResourceNotFoundError
    fn get_resource<T>(&self) -> Result<Arc<T>, ResourceNotFoundError>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
    {
        self.try_resource::<T>()
            .ok_or_else(|| ResourceNotFoundError::new::<T>())
    }

}

pub trait ResourceModify: ResourceAccess {
    /// Set a resource, returning the previous value if one existed
    ///
    /// This is the primary method for adding or updating resources. If a resource
    /// of the same type already exists, it will be replaced and the old value returned.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use syzygy::prelude::*;
    ///
    /// # #[derive(Debug, Clone)]
    /// # struct AppModel { counter: i32 }
    /// # impl Model for AppModel {
    /// #     type Snapshot = Self;
    /// #     fn to_snapshot(&self) -> Self::Snapshot { self.clone() }
    /// # }
    /// let mut syzygy = Syzygy::builder()
    ///     .model(AppModel { counter: 0 })
    ///     .build();
    ///
    /// // Set a new resource
    /// let old_config = syzygy.set_resource("initial config".to_string());
    /// assert!(old_config.is_none());
    ///
    /// // Replace existing resource
    /// let old_config = syzygy.set_resource("updated config".to_string());
    /// assert_eq!(*old_config.unwrap(), "initial config");
    /// ```
    fn set_resource<T>(&self, value: T) -> Option<Arc<T>>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
    {
        let arc_value = Arc::new(value);
        let boxed_value = Box::new(arc_value.clone());
        
        let mut write_guard = self.resources()
            .write()
            .expect("Failed to acquire write lock");
        let map = Arc::make_mut(&mut write_guard);
        let old_boxed = map.insert(TypeId::of::<T>(), boxed_value);

        old_boxed.and_then(|old| old.as_any().downcast_ref::<Arc<T>>().cloned())
    }

    fn remove_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + std::fmt::Debug + 'static,
    {
        let mut write_guard = self
            .resources()
            .write()
            .expect("Failed to acquire write lock");
        let map = Arc::make_mut(&mut write_guard);
        let removed = map.remove(&TypeId::of::<T>());

        removed.and_then(|boxed| boxed.as_any().downcast_ref::<Arc<T>>().cloned())
    }


}
