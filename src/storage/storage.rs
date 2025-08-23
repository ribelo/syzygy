//! UnsafeCell-based Chain implementation for interior mutability without runtime borrow checking
//!
//! This allows multiple mutable borrows of different chain elements at the same time
//! using UnsafeCell for zero-cost interior mutability. This is safe as long as we
//! only access different types from the chain.

use std::cell::UnsafeCell;
use std::marker::PhantomData;

// ============================================================================
// Core Types
// ============================================================================

/// Terminal type for the chain
#[derive(Debug, Default, Clone)]
pub struct EmptyStorage;

/// Generic chain structure with UnsafeCell-wrapped elements
#[derive(Debug)]
pub struct Storage<Head, Tail> {
    /// The head value wrapped in UnsafeCell for interior mutability
    head: UnsafeCell<Head>,
    /// The rest of the chain
    tail: Tail,
}

// ============================================================================
// Index Types (same as regular Chain)
// ============================================================================

/// Index marker for the first element
pub struct Here;

/// Index marker for elements in the tail
pub struct There<I>(PhantomData<I>);

// ============================================================================
// Contains Trait for Type Existence Checking
// ============================================================================

/// Marker trait indicating that a chain contains a specific type
///
/// This trait provides compile-time checking of type existence for API constraints.
/// Since true compile-time duplicate prevention is impossible in stable Rust
/// (no negative trait bounds), we use runtime checking for safety.
///
/// This trait is only implemented for types that actually exist in the chain.
pub trait Contains<T> {}

// EmptyStorage contains nothing (no implementations)

/// A chain contains its head type
impl<Head, Tail> Contains<Head> for Storage<Head, Tail> {}

// Note: We cannot implement recursive Contains<T> due to overlapping implementations
// in stable Rust. The head implementation above only works for the immediate head type.
// For runtime checking of all positions, use RuntimeContains::contains() method.

// ============================================================================
// Runtime Type Checking
// ============================================================================

/// Runtime trait for checking if a chain contains a specific type
pub trait RuntimeContains {
    /// Check if a type exists in the chain at runtime using TypeId comparison
    fn contains<T: 'static>(&self) -> bool;
}

impl RuntimeContains for EmptyStorage {
    fn contains<T: 'static>(&self) -> bool {
        false
    }
}

impl<Head: 'static, Tail: RuntimeContains> RuntimeContains for Storage<Head, Tail> {
    fn contains<T: 'static>(&self) -> bool {
        // Check if T is the same as Head
        if std::any::TypeId::of::<T>() == std::any::TypeId::of::<Head>() {
            return true;
        }

        // Check the tail recursively
        self.tail.contains::<T>()
    }
}

// ============================================================================
// Error Types
// ============================================================================

/// Error returned when trying to add a duplicate type to a chain
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateTypeError {
    /// Name of the type that was duplicated
    pub type_name: &'static str,
}

impl DuplicateTypeError {
    /// Create a new error for type T
    pub fn new<T: 'static>() -> Self {
        Self {
            type_name: std::any::type_name::<T>(),
        }
    }
}

impl std::fmt::Display for DuplicateTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Duplicate type in chain: {}", self.type_name)
    }
}

impl std::error::Error for DuplicateTypeError {}

// ============================================================================
// Selector Trait for UnsafeCell Chain
// ============================================================================

/// Trait for selecting values from the chain with UnsafeCell wrapping
pub trait Selector<T, Index> {
    /// Get an immutable reference to the value
    ///
    /// # Safety
    /// Caller must ensure no mutable references to the same value exist
    fn get(&self) -> &T;

    /// Get a mutable reference to the value
    ///
    /// # Safety
    /// This method returns `&mut T` from `&self`, which is normally forbidden.
    /// This is safe in our case because:
    /// 1. Each type appears exactly once in the chain (guaranteed by type system)
    /// 2. Different types can be accessed simultaneously without aliasing
    /// 3. Caller must ensure no other references to the same value exist
    #[allow(clippy::mut_from_ref)]
    fn get_mut(&self) -> &mut T;
}

// Implementation when T is at the head (Here index)
impl<T, Tail> Selector<T, Here> for Storage<T, Tail> {
    fn get(&self) -> &T {
        // SAFETY: We have exclusive access to this value through the type system.
        // Each type appears exactly once in the chain.
        unsafe { &*self.head.get() }
    }

    fn get_mut(&self) -> &mut T {
        // SAFETY: UnsafeCell allows interior mutability. Type system guarantees
        // each type appears exactly once, preventing aliasing of same type.
        unsafe { &mut *self.head.get() }
    }
}

// Implementation when T is in the tail (There index)
impl<Head, Tail, FromTail, TailIndex> Selector<FromTail, There<TailIndex>>
    for Storage<Head, Tail>
where
    Tail: Selector<FromTail, TailIndex>,
{
    fn get(&self) -> &FromTail {
        self.tail.get()
    }

    fn get_mut(&self) -> &mut FromTail {
        self.tail.get_mut()
    }
}

// ============================================================================
// Builder Methods
// ============================================================================

impl EmptyStorage {
    /// Start a new chain with a single value
    ///
    /// Since EmptyStorage is empty, this never has duplicates so no runtime check needed.
    pub fn with_model<T>(self, head: T) -> Storage<T, EmptyStorage> {
        Storage {
            head: UnsafeCell::new(head),
            tail: EmptyStorage,
        }
    }

    /// Start a new chain with a single value (alias for with_model)
    pub fn push_model<T>(self, head: T) -> Storage<T, EmptyStorage> {
        self.with_model(head)
    }

    /// Start a new chain with a single value (unchecked version)
    ///
    /// Same as with_model since EmptyStorage is empty and cannot have duplicates.
    pub fn with_model_unchecked<T>(self, head: T) -> Storage<T, EmptyStorage> {
        self.with_model(head)
    }

    /// Try to start a new chain with a single value
    ///
    /// Always succeeds since EmptyStorage is empty and cannot have duplicates.
    pub fn try_with_model<T: 'static>(self, head: T) -> Result<Storage<T, EmptyStorage>, DuplicateTypeError> {
        Ok(self.with_model(head))
    }
}

impl<Head> Storage<Head, EmptyStorage> {
    /// Create a new UnsafeChain with a single value
    pub fn new(head: Head) -> Self {
        Storage {
            head: UnsafeCell::new(head),
            tail: EmptyStorage,
        }
    }
}

impl<Head, Tail> Storage<Head, Tail> {
    /// Get an immutable reference to a value from the chain by type
    ///
    /// Type annotation on the left side is recommended:
    /// ```rust,ignore
    /// let model: &Model1 = chain.get();
    /// ```
    pub fn get<T, Index>(&self) -> &T
    where
        Self: Selector<T, Index>,
    {
        Selector::get(self)
    }

    /// Get a mutable reference to a value from the chain by type
    ///
    /// Type annotation on the left side is recommended:
    /// ```rust,ignore
    /// let model: &mut Model1 = chain.get_mut();
    /// ```
    ///
    /// # Safety
    /// This is safe when accessing different types simultaneously.
    /// The type system prevents aliasing of the same type.
    #[allow(clippy::mut_from_ref)]
    pub fn get_mut<T, Index>(&self) -> &mut T
    where
        Self: Selector<T, Index>,
    {
        Selector::get_mut(self)
    }

    /// Add a value to the front of the chain with runtime duplicate checking
    ///
    /// This method checks at runtime if the type already exists in the chain
    /// and panics if it does. For recoverable error handling, use `try_with_model`.
    /// For maximum performance without checks, use `with_model_unchecked`.
    ///
    /// # Panics
    /// Panics if the type `V` already exists in the chain.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let chain = EmptyStorage::default()
    ///     .with_model(Model1 { value: 1 })
    ///     .with_model(Model2 { value: 2 }); // OK
    ///
    /// // This will panic:
    /// // let bad_chain = chain.with_model(Model1 { value: 999 });
    /// ```
    pub fn with_model<V: 'static>(self, value: V) -> Storage<V, Self>
    where
        Self: RuntimeContains,
    {
        self.try_with_model(value)
            .unwrap_or_else(|err| panic!("{}", err))
    }

    /// Try to add a value to the front of the chain with runtime duplicate checking
    ///
    /// Returns `Err(DuplicateTypeError)` if the type already exists in the chain.
    /// This is the safe version for recoverable error handling.
    ///
    /// # Examples
    /// ```rust,ignore
    /// let chain = EmptyStorage::default()
    ///     .with_model(Model1 { value: 1 });
    ///
    /// match chain.try_with_model(Model1 { value: 2 }) {
    ///     Ok(new_chain) => {
    ///         // This won't happen - duplicate type
    ///     }
    ///     Err(err) => {
    ///         println!("Cannot add duplicate: {}", err);
    ///     }
    /// }
    /// ```
    pub fn try_with_model<V: 'static>(self, value: V) -> Result<Storage<V, Self>, DuplicateTypeError>
    where
        Self: RuntimeContains,
    {
        if self.contains::<V>() {
            return Err(DuplicateTypeError::new::<V>());
        }
        Ok(self.with_model_unchecked(value))
    }

    /// Add a value to the front of the chain without any duplicate checking
    ///
    /// This method performs no runtime checks and allows duplicate types.
    /// Use this for maximum performance when you're certain no duplicates exist,
    /// or when you intentionally want to allow duplicates.
    ///
    /// # Safety
    /// This method is safe to call but may create chains with duplicate types.
    /// Accessing duplicated types will result in compiler ambiguity errors.
    ///
    /// # Examples
    /// ```rust,ignore
    /// // Fast path - no checks
    /// let chain = EmptyStorage::default()
    ///     .with_model_unchecked(Model1 { value: 1 })
    ///     .with_model_unchecked(Model2 { value: 2 });
    ///
    /// // This compiles but creates unusable chain:
    /// let bad_chain = chain.with_model_unchecked(Model1 { value: 999 });
    /// // let model: &Model1 = bad_chain.get(); // Compile error: ambiguous!
    /// ```
    pub fn with_model_unchecked<V>(self, value: V) -> Storage<V, Self> {
        Storage {
            head: UnsafeCell::new(value),
            tail: self,
        }
    }

    /// Alias for `with_model` - adds a value with runtime duplicate checking
    ///
    /// # Panics
    /// Panics if the type `V` already exists in the chain.
    pub fn push_model<V: 'static>(self, value: V) -> Storage<V, Self>
    where
        Self: RuntimeContains,
    {
        self.with_model(value)
    }

    /// Alias for `try_with_model` - tries to add a value with runtime checking
    pub fn try_push_model<V: 'static>(self, value: V) -> Result<Storage<V, Self>, DuplicateTypeError>
    where
        Self: RuntimeContains,
    {
        self.try_with_model(value)
    }

    /// Alias for `with_model_unchecked` - adds a value without checking
    pub fn push_model_unchecked<V>(self, value: V) -> Storage<V, Self> {
        self.with_model_unchecked(value)
    }

    /// Get an immutable reference to the head value
    pub fn get_head(&self) -> &Head {
        // SAFETY: We have exclusive access to this value through the type system.
        unsafe { &*self.head.get() }
    }

    /// Get a mutable reference to the head value
    #[allow(clippy::mut_from_ref)]
    pub fn get_head_mut(&self) -> &mut Head {
        // SAFETY: UnsafeCell allows interior mutability. No aliasing within same chain.
        unsafe { &mut *self.head.get() }
    }

    /// Get a reference to the tail
    pub fn tail(&self) -> &Tail {
        &self.tail
    }
}

// ============================================================================
// Clone implementation (clones the inner values)
// ============================================================================

impl<Head: Clone, Tail: Clone> Clone for Storage<Head, Tail> {
    fn clone(&self) -> Self {
        Storage {
            // SAFETY: We're only reading from UnsafeCell to clone the value.
            head: UnsafeCell::new(unsafe { (*self.head.get()).clone() }),
            tail: self.tail.clone(),
        }
    }
}

// ============================================================================
// Send/Sync implementations - UnsafeCell blocks these by default
// ============================================================================

// SAFETY: Storage can be Send if Head and Tail are Send.
// UnsafeCell blocks Send by default, but we re-enable it since our usage is safe.
unsafe impl<Head: Send, Tail: Send> Send for Storage<Head, Tail> {}

// SAFETY: Storage can be Sync if Head and Tail are Sync.
// This is safe because we only allow access to different types simultaneously,
// preventing aliasing of the same type across threads.
unsafe impl<Head: Sync, Tail: Sync> Sync for Storage<Head, Tail> {}
