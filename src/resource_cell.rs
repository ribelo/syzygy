use std::fmt;
use std::sync::{Arc, RwLock};

/// Thread-safe cell for lazily initialized resources.
///
/// Internally this is just an `Arc<RwLock<Option<T>>>`, but the helper keeps the
/// intent obvious and prevents copy/pasting locking boilerplate across codebases.
#[derive(Debug, Clone)]
pub struct ResourceCell<T> {
    inner: Arc<RwLock<Option<T>>>,
}

impl<T> ResourceCell<T> {
    /// Create an empty cell.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
        }
    }

    /// Create a cell that already contains a value.
    #[must_use]
    pub fn new(value: T) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Some(value))),
        }
    }

    /// Returns `true` when the cell does not currently hold a value.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.read().expect("lock poisoned").is_none()
    }

    /// Replace the current value and return the previous one, if any.
    pub fn set(&self, value: T) -> Option<T> {
        self.inner.write().expect("lock poisoned").replace(value)
    }

    /// Set the value only if the cell is currently empty.
    ///
    /// Returns [`SetOnceError`] if a value was already present.
    pub fn set_once(&self, value: T) -> Result<(), SetOnceError> {
        let mut guard = self.inner.write().expect("lock poisoned");
        if guard.is_some() {
            return Err(SetOnceError);
        }
        *guard = Some(value);
        Ok(())
    }

    /// Take the value out of the cell, leaving it empty.
    pub fn take(&self) -> Option<T> {
        self.inner.write().expect("lock poisoned").take()
    }

    /// Run a closure with a shared reference to the value if it exists.
    ///
    /// This avoids exposing locking internals while still permitting read access.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        let guard = self.inner.read().expect("lock poisoned");
        guard.as_ref().map(f)
    }

    /// Run a closure with a mutable reference to the value if it exists.
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> Option<R> {
        let mut guard = self.inner.write().expect("lock poisoned");
        guard.as_mut().map(f)
    }

    /// Clone the inner value if it is present.
    pub fn cloned(&self) -> Option<T>
    where
        T: Clone,
    {
        self.inner.read().expect("lock poisoned").clone()
    }

    /// Expose the raw `Arc<RwLock<Option<T>>>` for integration with existing code.
    #[must_use]
    pub fn into_inner(self) -> Arc<RwLock<Option<T>>> {
        self.inner
    }
}

/// Error type returned when [`ResourceCell::set_once`] is called on a populated cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetOnceError;

impl fmt::Display for SetOnceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("resource cell already initialised")
    }
}

impl std::error::Error for SetOnceError {}
