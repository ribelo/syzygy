use rustc_hash::FxHashMap;
use std::any::TypeId;
use std::sync::Arc;

use super::{AsyncExecutor, SyncExecutor};

/// Registry of executors keyed by `TypeId` markers with separate async and sync capabilities.
pub struct ExecutorRegistry<E> {
    async_map: FxHashMap<TypeId, Arc<dyn AsyncExecutor<E>>>,
    sync_map: FxHashMap<TypeId, Arc<dyn SyncExecutor<E>>>,
}

impl<E> Default for ExecutorRegistry<E>
where
    E: Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E> ExecutorRegistry<E>
where
    E: Send + 'static,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            async_map: FxHashMap::default(),
            sync_map: FxHashMap::default(),
        }
    }

    /// Insert an async executor using concrete type T as key.
    pub fn insert_async<T>(&mut self, exec: T)
    where
        T: AsyncExecutor<E> + 'static,
    {
        self.async_map.insert(
            TypeId::of::<T>(),
            Arc::new(exec) as Arc<dyn AsyncExecutor<E>>,
        );
    }

    /// Insert a sync executor using concrete type T as key.
    pub fn insert_sync<T>(&mut self, exec: T)
    where
        T: SyncExecutor<E> + 'static,
    {
        self.sync_map.insert(
            TypeId::of::<T>(),
            Arc::new(exec) as Arc<dyn SyncExecutor<E>>,
        );
    }

    /// Get an async executor by marker type.
    #[must_use]
    pub fn async_exec<T: 'static>(&self) -> Option<Arc<dyn AsyncExecutor<E>>> {
        self.async_map.get(&TypeId::of::<T>()).cloned()
    }

    /// Get a sync executor by marker type.
    #[must_use]
    pub fn sync_exec<T: 'static>(&self) -> Option<Arc<dyn SyncExecutor<E>>> {
        self.sync_map.get(&TypeId::of::<T>()).cloned()
    }

    /// Get an async executor by raw `TypeId` key.
    #[must_use]
    pub(crate) fn async_exec_by_key(&self, key: TypeId) -> Option<Arc<dyn AsyncExecutor<E>>> {
        self.async_map.get(&key).cloned()
    }

    /// Get a sync executor by raw `TypeId` key.
    #[must_use]
    pub(crate) fn sync_exec_by_key(&self, key: TypeId) -> Option<Arc<dyn SyncExecutor<E>>> {
        self.sync_map.get(&key).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::executor::{SingleThreadExecutor, TokioExecutor};

    #[derive(Debug, Clone)]
    #[allow(dead_code)]
    enum TestEvent {
        Done,
    }

    // Marker types for testing type safety
    #[allow(dead_code)]
    struct IoKey;
    #[allow(dead_code)]
    struct CpuKey;

    fn setup_registry_with_executors() -> ExecutorRegistry<TestEvent> {
        let mut reg = ExecutorRegistry::default();
        reg.insert_async(TokioExecutor::current_thread_io("test"));
        reg.insert_sync(SingleThreadExecutor::new());
        reg
    }

    #[test]
    fn registry_stores_and_retrieves_async_executor_by_type() {
        // Given: A registry with executors
        let registry = setup_registry_with_executors();

        // When: Retrieve async executor by type
        let async_exec = registry.async_exec::<TokioExecutor>();

        // Then: Correct executor is returned
        assert!(async_exec.is_some(), "TokioExecutor should be found");
    }

    #[test]
    fn registry_stores_and_retrieves_sync_executor_by_type() {
        // Given: A registry with executors
        let registry = setup_registry_with_executors();

        // When: Retrieve sync executor by type
        let sync_exec = registry.sync_exec::<SingleThreadExecutor>();

        // Then: Correct executor is returned
        assert!(sync_exec.is_some(), "SingleThreadExecutor should be found");
    }

    #[test]
    fn registry_returns_none_for_unregistered_executor_types() {
        // Given: A registry with executors
        let registry = setup_registry_with_executors();

        // When: Try to get executor type that wasn't registered
        let missing_exec = registry.async_exec::<SingleThreadExecutor>(); // Wrong category

        // Then: None is returned
        assert!(
            missing_exec.is_none(),
            "Should not find async executor for sync type"
        );
    }

    #[test]
    fn registry_handles_empty_registry_gracefully() {
        // Given: An empty registry
        let registry = ExecutorRegistry::<TestEvent>::default();

        // When: Query for any executor
        let async_exec = registry.async_exec::<TokioExecutor>();
        let sync_exec = registry.sync_exec::<SingleThreadExecutor>();

        // Then: No executors found
        assert!(
            async_exec.is_none(),
            "Empty registry should not have async executors"
        );
        assert!(
            sync_exec.is_none(),
            "Empty registry should not have sync executors"
        );
    }

    #[test]
    fn registry_overwrites_executor_when_same_type_inserted_twice() {
        // Given: A registry with an initial executor
        let mut registry = ExecutorRegistry::<TestEvent>::default();
        registry.insert_async(TokioExecutor::current_thread_io("first"));

        // When: Insert another executor of the same type
        registry.insert_async(TokioExecutor::current_thread_io("second"));

        // Then: The second executor overwrites the first
        let exec = registry.async_exec::<TokioExecutor>();
        assert!(exec.is_some(), "Executor should be present after overwrite");
    }

    #[test]
    fn registry_async_exec_by_key_provides_raw_typeid_access_for_internal_use() {
        // Given: A registry with executors
        let registry = setup_registry_with_executors();

        // When: Use raw TypeId access (internal API)
        let key = TypeId::of::<TokioExecutor>();
        let exec = registry.async_exec_by_key(key);

        // Then: Executor is found
        assert!(exec.is_some(), "Raw TypeId access should find executor");
    }

    #[test]
    fn registry_sync_exec_by_key_provides_raw_typeid_access_for_internal_use() {
        // Given: A registry with executors
        let registry = setup_registry_with_executors();

        // When: Use raw TypeId access (internal API)
        let key = TypeId::of::<SingleThreadExecutor>();
        let exec = registry.sync_exec_by_key(key);

        // Then: Executor is found
        assert!(exec.is_some(), "Raw TypeId access should find executor");
    }
}
