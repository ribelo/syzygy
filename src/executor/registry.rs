use rustc_hash::FxHashMap;
use std::any::TypeId;
use std::marker::PhantomData;
use std::sync::Arc;

use super::{AsyncExecutor, BlockingExecutor, ResourceBlockingExecutor};

#[allow(clippy::struct_field_names)]
pub struct ExecutorRegistry<E> {
    async_map: FxHashMap<TypeId, Arc<dyn AsyncExecutor>>,
    blocking_map: FxHashMap<TypeId, Arc<dyn BlockingExecutor>>,
    resource_blocking_map: FxHashMap<TypeId, Arc<dyn ResourceBlockingExecutor>>,
    marker: PhantomData<fn() -> E>,
}

impl<E> Default for ExecutorRegistry<E>
where
    E: Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E> ExecutorRegistry<E>
where
    E: Send + Sync + 'static,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            async_map: FxHashMap::default(),
            blocking_map: FxHashMap::default(),
            resource_blocking_map: FxHashMap::default(),
            marker: PhantomData,
        }
    }

    pub fn insert_async<T>(&mut self, exec: T)
    where
        T: AsyncExecutor + Send + Sync + 'static,
    {
        let key = TypeId::of::<T>();
        let erased: Arc<dyn AsyncExecutor + Send + Sync> = Arc::new(exec);
        self.async_map.insert(key, erased);
    }

    pub fn insert_blocking<T>(&mut self, exec: T)
    where
        T: BlockingExecutor + Send + Sync + 'static,
    {
        let key = TypeId::of::<T>();
        let erased: Arc<dyn BlockingExecutor + Send + Sync> = Arc::new(exec);
        self.blocking_map.insert(key, erased);
    }

    pub fn insert_resource_blocking<T>(&mut self, exec: T)
    where
        T: ResourceBlockingExecutor + Send + Sync + 'static,
    {
        let key = TypeId::of::<T>();
        let erased: Arc<dyn ResourceBlockingExecutor + Send + Sync> = Arc::new(exec);
        self.resource_blocking_map.insert(key, erased);
    }

    #[must_use]
    pub fn async_exec<T: 'static>(&self) -> Option<Arc<dyn AsyncExecutor>> {
        self.async_map.get(&TypeId::of::<T>()).cloned()
    }

    #[must_use]
    pub fn blocking_exec<T: 'static>(&self) -> Option<Arc<dyn BlockingExecutor>> {
        self.blocking_map.get(&TypeId::of::<T>()).cloned()
    }

    #[must_use]
    pub fn resource_blocking_exec<T: 'static>(&self) -> Option<Arc<dyn ResourceBlockingExecutor>> {
        self.resource_blocking_map.get(&TypeId::of::<T>()).cloned()
    }

    pub(crate) fn async_exec_by_key(&self, key: TypeId) -> Option<Arc<dyn AsyncExecutor>> {
        self.async_map.get(&key).cloned()
    }

    pub(crate) fn blocking_exec_by_key(&self, key: TypeId) -> Option<Arc<dyn BlockingExecutor>> {
        self.blocking_map.get(&key).cloned()
    }

    pub(crate) fn resource_blocking_exec_by_key(
        &self,
        key: TypeId,
    ) -> Option<Arc<dyn ResourceBlockingExecutor>> {
        self.resource_blocking_map.get(&key).cloned()
    }

    pub fn shutdown_all(&self) {
        for exec in self.async_map.values() {
            exec.shutdown();
        }

        for exec in self.blocking_map.values() {
            exec.shutdown();
        }

        for exec in self.resource_blocking_map.values() {
            exec.shutdown();
        }
    }

    pub fn wait_all(&self) {
        for exec in self.async_map.values() {
            exec.wait();
        }

        for exec in self.blocking_map.values() {
            exec.wait();
        }

        for exec in self.resource_blocking_map.values() {
            exec.wait();
        }
    }
}
