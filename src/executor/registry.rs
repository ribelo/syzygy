use rustc_hash::FxHashMap;
use std::any::TypeId;
use std::sync::Arc;

use futures_util::future::join_all;

use super::{
    AsyncOwnedExecutor, DynAsyncExecutor, DynAsyncExecutorAdapter, DynSyncBorrowedExecutor,
    DynSyncBorrowedExecutorAdapter, DynSyncOwnedExecutor, DynSyncOwnedExecutorAdapter,
    ExecutorLifecycle, SyncBorrowedExecutor, SyncOwnedExecutor,
};

pub struct ExecutorRegistry<E, X> {
    async_map: FxHashMap<TypeId, Arc<dyn DynAsyncExecutor<E, X>>>,
    sync_borrowed_map: FxHashMap<TypeId, Arc<dyn DynSyncBorrowedExecutor<E, X>>>,
    sync_owned_map: FxHashMap<TypeId, Arc<dyn DynSyncOwnedExecutor<E, X>>>,
}

impl<E, X> Default for ExecutorRegistry<E, X>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E, X> ExecutorRegistry<E, X>
where
    E: Send + Sync + 'static,
    X: Send + Sync + 'static,
{
    #[must_use]
    pub fn new() -> Self {
        Self {
            async_map: FxHashMap::default(),
            sync_borrowed_map: FxHashMap::default(),
            sync_owned_map: FxHashMap::default(),
        }
    }

    pub fn insert_async<T>(&mut self, exec: T)
    where
        T: AsyncOwnedExecutor<E> + 'static,
    {
        let key = TypeId::of::<T>();
        let erased: Arc<dyn DynAsyncExecutor<E, X>> =
            Arc::new(DynAsyncExecutorAdapter::new(Arc::new(exec)));
        self.async_map.insert(key, erased);
    }

    pub fn insert_sync_borrowed<T>(&mut self, exec: T)
    where
        T: SyncBorrowedExecutor<E> + 'static,
    {
        let key = TypeId::of::<T>();
        let erased: Arc<dyn DynSyncBorrowedExecutor<E, X>> =
            Arc::new(DynSyncBorrowedExecutorAdapter::new(Arc::new(exec)));
        self.sync_borrowed_map.insert(key, erased);
    }

    pub fn insert_sync_owned<T>(&mut self, exec: T)
    where
        T: SyncOwnedExecutor<E> + 'static,
    {
        let key = TypeId::of::<T>();
        let erased: Arc<dyn DynSyncOwnedExecutor<E, X>> =
            Arc::new(DynSyncOwnedExecutorAdapter::new(Arc::new(exec)));
        self.sync_owned_map.insert(key, erased);
    }

    #[must_use]
    pub fn async_exec<T: 'static>(&self) -> Option<Arc<dyn DynAsyncExecutor<E, X>>> {
        self.async_map.get(&TypeId::of::<T>()).cloned()
    }

    #[must_use]
    pub fn sync_borrowed_exec<T: 'static>(&self) -> Option<Arc<dyn DynSyncBorrowedExecutor<E, X>>> {
        self.sync_borrowed_map.get(&TypeId::of::<T>()).cloned()
    }

    pub(crate) fn async_exec_by_key(&self, key: TypeId) -> Option<Arc<dyn DynAsyncExecutor<E, X>>> {
        self.async_map.get(&key).cloned()
    }

    pub(crate) fn sync_borrowed_exec_by_key(
        &self,
        key: TypeId,
    ) -> Option<Arc<dyn DynSyncBorrowedExecutor<E, X>>> {
        self.sync_borrowed_map.get(&key).cloned()
    }

    pub(crate) fn sync_owned_exec_by_key(
        &self,
        key: TypeId,
    ) -> Option<Arc<dyn DynSyncOwnedExecutor<E, X>>> {
        self.sync_owned_map.get(&key).cloned()
    }

    pub fn set_default_async<T: 'static>(&mut self) -> bool {
        self.async_map.contains_key(&TypeId::of::<T>())
    }

    #[must_use]
    pub fn has_default_async(&self) -> bool {
        !self.async_map.is_empty()
    }

    #[must_use]
    pub fn default_async(&self) -> Option<Arc<dyn DynAsyncExecutor<E, X>>> {
        self.async_map.values().next().cloned()
    }

    fn all_executors(&self) -> impl Iterator<Item = Arc<dyn ExecutorLifecycle>> + '_ {
        self.async_map
            .values()
            .cloned()
            .map(|exec| exec as Arc<dyn ExecutorLifecycle>)
            .chain(
                self.sync_borrowed_map
                    .values()
                    .cloned()
                    .map(|exec| exec as Arc<dyn ExecutorLifecycle>),
            )
            .chain(
                self.sync_owned_map
                    .values()
                    .cloned()
                    .map(|exec| exec as Arc<dyn ExecutorLifecycle>),
            )
    }

    pub fn shutdown_all(&self) {
        for exec in self.all_executors() {
            exec.shutdown();
        }
    }

    pub async fn join_all(&self) {
        let futures: Vec<_> = self.all_executors().map(|exec| exec.join()).collect();
        join_all(futures).await;
    }
}
