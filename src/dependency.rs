use std::any::{Any, TypeId};
use std::ops::Deref;
use std::sync::Arc;

use rustc_hash::FxHashMap;

#[derive(Default, Clone)]
pub struct ResourceMap {
    inner: FxHashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl ResourceMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) {
        self.inner.insert(TypeId::of::<T>(), Arc::new(value));
    }

    #[must_use]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.inner
            .get(&TypeId::of::<T>())
            .cloned()
            .map(|arc| arc.downcast::<T>().expect("TypeId mismatch"))
    }

    #[must_use]
    pub fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.inner.contains_key(&TypeId::of::<T>())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl std::fmt::Debug for ResourceMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceMap")
            .field("count", &self.inner.len())
            .finish()
    }
}

/// Extractor for typed resources from the ResourceMap.
/// Use in effect handler signatures: `fn handle(payload: P, api: Res<ApiClient>) -> Task<E, X>`
pub struct Res<T>(Arc<T>);

impl<T> Deref for Res<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Clone for Res<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Res<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> Res<T> {
    #[must_use]
    pub fn into_arc(self) -> Arc<T> {
        self.0
    }

    pub(crate) fn from_arc(inner: Arc<T>) -> Self {
        Self(inner)
    }
}
