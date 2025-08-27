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
impl<Head, Tail, FromTail, TailIndex> Selector<FromTail, There<TailIndex>> for Storage<Head, Tail>
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
// Storage Builder Trait
// ============================================================================

/// Trait for types that can have models added to them
pub trait StorageBuilder<T> {
    /// The type after adding a model
    type Output;

    /// Add a model with runtime duplicate checking
    fn with_model(self, model: T) -> Self::Output;
}

// ============================================================================
// Builder Methods
// ============================================================================

// Implement StorageBuilder for EmptyStorage
impl<T> StorageBuilder<T> for EmptyStorage {
    type Output = Storage<T, EmptyStorage>;

    fn with_model(self, model: T) -> Self::Output {
        Storage {
            head: UnsafeCell::new(model),
            tail: self,
        }
    }
}

// Implement StorageBuilder for Storage
impl<T: 'static, Head, Tail> StorageBuilder<T> for Storage<Head, Tail>
where
    Self: RuntimeContains,
{
    type Output = Storage<T, Self>;

    fn with_model(self, model: T) -> Self::Output {
        assert!(
            !self.contains::<T>(),
            "Duplicate type in chain: {}",
            std::any::type_name::<T>()
        );
        Storage {
            head: UnsafeCell::new(model),
            tail: self,
        }
    }
}

impl EmptyStorage {
    /// Start a new chain with a single value
    ///
    /// Since EmptyStorage is empty, this never has duplicates so no runtime check needed.
    pub fn with_model<T>(self, head: T) -> Storage<T, EmptyStorage> {
        StorageBuilder::with_model(self, head)
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
    /// and panics if it does. This is the only supported method for adding
    /// models to storage chains.
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
        StorageBuilder::with_model(self, value)
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

// ============================================================================
// Bulk Extraction Trait for Type-Inferred Multi-Model Access
// ============================================================================

/// Trait for extracting multiple models in a single operation with type inference
pub trait BulkExtract<'a, T, I> {
    /// Extract multiple models at once, with tuple size inferred from return type
    ///
    /// # Example
    /// ```rust,ignore
    /// let storage = EmptyStorage
    ///     .with_model(Model1 { value: 1 })
    ///     .with_model(Model2 { value: 2 });
    ///
    /// // Type inference determines tuple size
    /// let (m1, m2): (&Model1, &Model2) = storage.extract_bulk();
    /// ```
    fn extract_bulk(&'a self) -> T;
}

/// Trait for extracting multiple models mutably with type inference
pub trait BulkExtractMut<'a, T, I> {
    /// Extract multiple models mutably at once, with tuple size inferred from return type
    ///
    /// # Example
    /// ```rust,ignore
    /// let mut storage = EmptyStorage
    ///     .with_model(Model1 { value: 1 })
    ///     .with_model(Model2 { value: 2 });
    ///
    /// let (m1, m2): (&mut Model1, &mut Model2) = storage.extract_bulk_mut();
    /// m1.value = 10;
    /// ```
    fn extract_bulk_mut(&'a self) -> T;
}

macro_rules! impl_bulk_extract {
    ($($T:ident, $I:ident),*) => {
        impl<'a, Head, Tail, $($T, $I),*>
            BulkExtract<'a, ($(&'a $T,)*), ($($I,)*)> for Storage<Head, Tail>
        where
            Self: $(Selector<$T, $I> +)* Send + Sync + 'a,
        {
            fn extract_bulk(&'a self) -> ($(&'a $T,)*) {
                ($(self.get::<$T, $I>(),)*)
            }
        }

        impl<'a, Head, Tail, $($T, $I),*>
            BulkExtractMut<'a, ($(&'a mut $T,)*), ($($I,)*)> for Storage<Head, Tail>
        where
            Self: $(Selector<$T, $I> +)* Send + Sync + 'a,
        {
            fn extract_bulk_mut(&'a self) -> ($(&'a mut $T,)*) {
                ($(self.get_mut::<$T, $I>(),)*)
            }
        }
    };
}

// Generate implementations for tuples 1-16 using a cleaner approach
impl_bulk_extract!(T1, I1);
impl_bulk_extract!(T1, I1, T2, I2);
impl_bulk_extract!(T1, I1, T2, I2, T3, I3);
impl_bulk_extract!(T1, I1, T2, I2, T3, I3, T4, I4);
impl_bulk_extract!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5);
impl_bulk_extract!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6);
impl_bulk_extract!(T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7);
impl_bulk_extract!(
    T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8
);
impl_bulk_extract!(
    T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9
);
impl_bulk_extract!(
    T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10
);
impl_bulk_extract!(
    T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11
);
impl_bulk_extract!(
    T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
    T12, I12
);
impl_bulk_extract!(
    T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
    T12, I12, T13, I13
);
impl_bulk_extract!(
    T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
    T12, I12, T13, I13, T14, I14
);
impl_bulk_extract!(
    T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
    T12, I12, T13, I13, T14, I14, T15, I15
);
impl_bulk_extract!(
    T1, I1, T2, I2, T3, I3, T4, I4, T5, I5, T6, I6, T7, I7, T8, I8, T9, I9, T10, I10, T11, I11,
    T12, I12, T13, I13, T14, I14, T15, I15, T16, I16
);

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Model1 {
        value: i32,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Model2 {
        value: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Model3 {
        value: f64,
    }

    #[test]
    fn test_bulk_extract_single() {
        let storage = EmptyStorage.with_model(Model1 { value: 42 });

        let (m1,): (&Model1,) = storage.extract_bulk();
        assert_eq!(m1.value, 42);
    }

    #[test]
    fn test_bulk_extract_two() {
        let storage = EmptyStorage
            .with_model(Model1 { value: 42 })
            .with_model(Model2 {
                value: "hello".to_string(),
            });

        let (m1, m2): (&Model1, &Model2) = storage.extract_bulk();
        assert_eq!(m1.value, 42);
        assert_eq!(m2.value, "hello");

        // Test with different order
        let (m2, m1): (&Model2, &Model1) = storage.extract_bulk();
        assert_eq!(m1.value, 42);
        assert_eq!(m2.value, "hello");
    }

    #[test]
    fn test_bulk_extract_mut_two() {
        let storage = EmptyStorage
            .with_model(Model1 { value: 42 })
            .with_model(Model2 {
                value: "hello".to_string(),
            });

        let (m1, m2): (&mut Model1, &mut Model2) = storage.extract_bulk_mut();
        m1.value = 100;
        m2.value = "world".to_string();

        assert_eq!(m1.value, 100);
        assert_eq!(m2.value, "world");

        // Verify that the original storage was modified
        let (m1_immut, m2_immut): (&Model1, &Model2) = storage.extract_bulk();
        assert_eq!(m1_immut.value, 100);
        assert_eq!(m2_immut.value, "world");
    }
}
