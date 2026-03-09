use std::any::{Any, TypeId};

use rustc_hash::FxHashMap;

type CloneFn = fn(&dyn Any) -> Box<dyn Any>;

struct Entry {
    value: Box<dyn Any>,
    clone_fn: CloneFn,
}

#[derive(Default)]
pub struct ResourceMap {
    inner: FxHashMap<TypeId, Entry>,
}

impl Clone for ResourceMap {
    fn clone(&self) -> Self {
        let inner = self
            .inner
            .iter()
            .map(|(&tid, entry)| {
                let cloned_value = (entry.clone_fn)(&*entry.value);
                (
                    tid,
                    Entry {
                        value: cloned_value,
                        clone_fn: entry.clone_fn,
                    },
                )
            })
            .collect();
        Self { inner }
    }
}

fn clone_fn_for<T: Clone + 'static>(any: &dyn Any) -> Box<dyn Any> {
    Box::new(
        any.downcast_ref::<T>()
            .unwrap_or_else(|| panic_resource_type_mismatch::<T>())
            .clone(),
    )
}

fn panic_resource_type_mismatch<T: 'static>() -> ! {
    panic!(
        "ResourceMap internal type mismatch for `{}`. This indicates a programmer error in ResourceMap storage invariants.",
        std::any::type_name::<T>()
    )
}

impl ResourceMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert<T: Clone + 'static>(&mut self, value: T) {
        self.inner.insert(
            TypeId::of::<T>(),
            Entry {
                value: Box::new(value),
                clone_fn: clone_fn_for::<T>,
            },
        );
    }

    #[must_use]
    pub fn get<T: Clone + 'static>(&self) -> Option<T> {
        self.inner.get(&TypeId::of::<T>()).map(|entry| {
            entry
                .value
                .downcast_ref::<T>()
                .unwrap_or_else(|| panic_resource_type_mismatch::<T>())
                .clone()
        })
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

pub trait Resource: Clone + 'static {}
impl<T: Clone + 'static> Resource for T {}
