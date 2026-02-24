use std::any::{Any, TypeId};
use std::ops::Deref;
use std::rc::Rc;

use rustc_hash::FxHashMap;

#[derive(Default, Clone)]
pub struct ResourceMap {
    inner: FxHashMap<TypeId, Rc<dyn Any>>,
}

impl ResourceMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: 'static>(&mut self, value: T) {
        self.inner.insert(TypeId::of::<T>(), Rc::new(value));
    }

    #[must_use]
    pub fn get<T: 'static>(&self) -> Option<Rc<T>> {
        self.inner
            .get(&TypeId::of::<T>())
            .cloned()
            .and_then(|rc| rc.downcast::<T>().ok())
    }

    #[must_use]
    pub fn contains<T: 'static>(&self) -> bool {
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
pub struct Res<T>(Rc<T>);

impl<T> Deref for Res<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> Clone for Res<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for Res<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl<T> Res<T> {
    #[must_use]
    pub fn into_rc(self) -> Rc<T> {
        self.0
    }

    pub(crate) fn from_rc(inner: Rc<T>) -> Self {
        Self(inner)
    }
}
