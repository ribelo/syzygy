// Minimal storage system to satisfy builder/tests during refactor

/// Empty storage marker
#[derive(Clone, Copy, Default, Debug)]
pub struct EmptyStorage;

/// HList-like storage of models/resources
#[derive(Clone, Debug)]
pub struct Storage<Head, Tail = EmptyStorage> {
    pub head: Head,
    pub tail: Tail,
}

/// Builder trait to extend storage
pub trait StorageBuilder<T> {
    type Output;
    fn with_model(self, model: T) -> Self::Output;
}

impl<T> StorageBuilder<T> for EmptyStorage {
    type Output = Storage<T, EmptyStorage>;
    fn with_model(self, model: T) -> Self::Output {
        Storage {
            head: model,
            tail: EmptyStorage,
        }
    }
}

impl<Head, Tail, T> StorageBuilder<T> for Storage<Head, Tail> {
    type Output = Storage<T, Storage<Head, Tail>>;
    fn with_model(self, model: T) -> Self::Output {
        Storage {
            head: model,
            tail: self,
        }
    }
}

/// Selector trait to access items; only the head is supported in this minimal impl
pub trait Selector<T, Index> {
    fn get(&self) -> &T;
    fn get_mut(&mut self) -> &mut T;
}

impl<T, Tail> Selector<T, ()> for Storage<T, Tail> {
    fn get(&self) -> &T {
        &self.head
    }
    fn get_mut(&mut self) -> &mut T {
        &mut self.head
    }
}
