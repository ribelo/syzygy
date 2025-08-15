use std::{
    any::{Any, TypeId},
    ops::Deref,
    sync::{Arc, RwLock},
};

use rustc_hash::FxHashMap;

use crate::error::ResourceNotFoundError;

/// Copy-on-write resource storage with lock-minimal reads
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
pub struct Resources(Arc<RwLock<Arc<FxHashMap<TypeId, Arc<dyn Any + Send + Sync>>>>>);

impl Default for Resources {
    fn default() -> Self {
        Self(Arc::new(RwLock::new(Arc::new(FxHashMap::default()))))
    }
}

impl Deref for Resources {
    type Target = RwLock<Arc<FxHashMap<TypeId, Arc<dyn Any + Send + Sync>>>>;

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
        T: Send + Sync + 'static,
    {
        let ty = TypeId::of::<T>();
        let arc_value: Arc<dyn Any + Send + Sync> = Arc::new(value);

        let mut write_guard = self.write().expect("Failed to acquire write lock");
        let map = Arc::make_mut(&mut write_guard);
        map.insert(ty, arc_value);
    }

    /// Get a resource using lock-minimal read semantics
    ///
    /// This operation:
    /// 1. Acquires a read lock (fast, shared with other readers)
    /// 2. Clones the inner Arc<HashMap> (cheap atomic ref increment)
    /// 3. Releases the read lock immediately
    /// 4. Performs HashMap lookup with zero contention
    #[must_use]
    pub fn get<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
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
            .and_then(|arc_any| Arc::clone(arc_any).downcast::<T>().ok())
    }

    /// Check if a resource of type T exists without allocating
    #[must_use]
    pub fn contains<T>(&self) -> bool
    where
        T: Send + Sync + 'static,
    {
        let ty = TypeId::of::<T>();
        let map = {
            let read_guard = self.read().expect("Failed to acquire read lock");
            Arc::clone(&read_guard)
        };
        map.contains_key(&ty)
    }

    /// Insert a resource from an existing Arc to avoid extra allocation
    pub fn insert_arc<T>(&self, arc_value: Arc<T>)
    where
        T: Send + Sync + 'static,
    {
        let ty = TypeId::of::<T>();
        let arc_any: Arc<dyn Any + Send + Sync> = arc_value;

        let mut write_guard = self.write().expect("Failed to acquire write lock");
        let map = Arc::make_mut(&mut write_guard);
        map.insert(ty, arc_any);
    }
}

pub trait ResourceAccess {
    fn resources(&self) -> &Resources;
    fn resource<T>(&self) -> Arc<T>
    where
        T: Send + Sync + 'static,
    {
        self.resources()
            .get::<T>()
            .unwrap_or_else(|| panic!("Resource of type {} not found", std::any::type_name::<T>()))
    }
    fn try_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        self.resources().get::<T>()
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
        T: Send + Sync + 'static,
    {
        self.try_resource::<T>()
            .ok_or_else(|| ResourceNotFoundError::new::<T>())
    }

    /// Check if a resource of type T exists
    fn contains_resource<T>(&self) -> bool
    where
        T: Send + Sync + 'static,
    {
        self.resources().contains::<T>()
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
    /// ```rust,ignore
    /// use syzygy::prelude::*;
    /// use syzygy::resource::ResourceModify;
    ///
    /// // This example shows the intended API (trait not currently implemented)
    /// let syzygy = /* ... initialized Syzygy system ... */;
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
        T: Send + Sync + 'static,
    {
        let arc_value: Arc<dyn Any + Send + Sync> = Arc::new(value);

        let mut write_guard = self
            .resources()
            .write()
            .expect("Failed to acquire write lock");
        let map = Arc::make_mut(&mut write_guard);
        let old_arc = map.insert(TypeId::of::<T>(), arc_value);

        old_arc.and_then(|old| old.downcast::<T>().ok())
    }

    fn remove_resource<T>(&self) -> Option<Arc<T>>
    where
        T: Send + Sync + 'static,
    {
        let mut write_guard = self
            .resources()
            .write()
            .expect("Failed to acquire write lock");
        let map = Arc::make_mut(&mut write_guard);
        let removed = map.remove(&TypeId::of::<T>());

        removed.and_then(|arc| arc.downcast::<T>().ok())
    }
}
