use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};

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

    /// Insert a new resource for type `T`.
    ///
    /// Panics if a resource for `T` is already registered. Use
    /// [`replace`](Self::replace) when replacement is intentional.
    pub fn insert<T: Clone + 'static>(&mut self, value: T) {
        self.try_insert(value).unwrap_or_else(|_value| {
            panic!(
                "resource `{}` already registered; use ResourceMap::replace when replacement is intentional",
                std::any::type_name::<T>()
            )
        });
    }

    /// Insert a resource only if `T` is not already present.
    ///
    /// Returns `Err(value)` when a resource for `T` already exists.
    pub fn try_insert<T: Clone + 'static>(&mut self, value: T) -> Result<(), T> {
        if self.contains::<T>() {
            return Err(value);
        }

        self.inner.insert(
            TypeId::of::<T>(),
            Entry {
                value: Box::new(value),
                clone_fn: clone_fn_for::<T>,
            },
        );

        Ok(())
    }

    /// Replace the resource for `T`, returning the previous value if present.
    pub fn replace<T: Clone + 'static>(&mut self, value: T) -> Option<T> {
        let replaced = self.inner.insert(
            TypeId::of::<T>(),
            Entry {
                value: Box::new(value),
                clone_fn: clone_fn_for::<T>,
            },
        );

        replaced.map(|entry| {
            entry
                .value
                .downcast::<T>()
                .unwrap_or_else(|_| panic_resource_type_mismatch::<T>())
                .as_ref()
                .clone()
        })
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

    /// Borrow the resource registered for `T` without cloning it.
    #[must_use]
    pub fn get_ref<T: 'static>(&self) -> Option<&T> {
        self.inner.get(&TypeId::of::<T>()).map(|entry| {
            entry
                .value
                .downcast_ref::<T>()
                .unwrap_or_else(|| panic_resource_type_mismatch::<T>())
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

/// Untyped resource environment used by the existing runtime-checked API.
#[derive(Clone, Copy, Debug, Default)]
pub struct DynamicEnv;

/// Empty typed resource environment.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvNil;

/// Type-level resource environment node.
#[derive(Clone, Copy, Debug, Default)]
pub struct EnvCons<Head, Tail> {
    head: Head,
    tail: Tail,
}

impl<Head, Tail> EnvCons<Head, Tail> {
    #[must_use]
    pub fn new(head: Head, tail: Tail) -> Self {
        Self { head, tail }
    }
}

#[doc(hidden)]
pub trait TypedResourceEnv {}

impl TypedResourceEnv for EnvNil {}

impl<Head, Tail> TypedResourceEnv for EnvCons<Head, Tail> where Tail: TypedResourceEnv {}

/// Proof that a type is at the current environment position.
#[derive(Clone, Copy, Debug, Default)]
pub struct Here;

/// Proof that a type is in the tail environment.
#[derive(Clone, Copy, Debug, Default)]
pub struct There<Index>(std::marker::PhantomData<fn() -> Index>);

/// Compile-time proof that `Self` contains resource `T` at `Index`.
pub trait HasResource<T, Index> {
    fn get(&self) -> &T;
    fn get_mut(&mut self) -> &mut T;
}

impl<T, Tail> HasResource<T, Here> for EnvCons<T, Tail> {
    fn get(&self) -> &T {
        &self.head
    }

    fn get_mut(&mut self) -> &mut T {
        &mut self.head
    }
}

impl<Head, Tail, T, Index> HasResource<T, There<Index>> for EnvCons<Head, Tail>
where
    Tail: HasResource<T, Index>,
{
    fn get(&self) -> &T {
        self.tail.get()
    }

    fn get_mut(&mut self) -> &mut T {
        self.tail.get_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::{EnvCons, EnvNil, HasResource, Here, ResourceMap, There};

    #[test]
    fn try_insert_rejects_duplicate_type() {
        let mut resources = ResourceMap::new();
        assert!(resources.try_insert::<u32>(7).is_ok());

        let duplicate = resources.try_insert::<u32>(9);
        assert_eq!(duplicate, Err(9));
        assert_eq!(resources.get::<u32>(), Some(7));
    }

    #[test]
    fn replace_returns_previous_value() {
        let mut resources = ResourceMap::new();
        assert_eq!(resources.replace::<u32>(1), None);
        assert_eq!(resources.replace::<u32>(2), Some(1));
        assert_eq!(resources.get::<u32>(), Some(2));
    }

    #[test]
    fn insert_panics_on_duplicate_without_replace() {
        let mut resources = ResourceMap::new();
        resources.insert::<u32>(1);

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            resources.insert::<u32>(2);
        }))
        .expect_err("duplicate insert must panic");

        let message = if let Some(s) = panic.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = panic.downcast_ref::<&str>() {
            (*s).to_string()
        } else {
            String::new()
        };

        assert!(message.contains("already registered"));
        assert!(message.contains("ResourceMap::replace"));
    }

    #[test]
    fn has_resource_gets_head_and_tail_resources() {
        let mut env = EnvCons::new(7u32, EnvCons::new("db", EnvNil));

        assert_eq!(
            *<EnvCons<u32, EnvCons<&str, EnvNil>> as HasResource<u32, Here>>::get(&env),
            7
        );
        assert_eq!(
            *<EnvCons<u32, EnvCons<&str, EnvNil>> as HasResource<&str, There<Here>>>::get(&env),
            "db"
        );

        *<EnvCons<u32, EnvCons<&str, EnvNil>> as HasResource<u32, Here>>::get_mut(&mut env) = 9;
        assert_eq!(
            *<EnvCons<u32, EnvCons<&str, EnvNil>> as HasResource<u32, Here>>::get(&env),
            9
        );
    }
}
