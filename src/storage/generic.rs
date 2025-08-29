//! Generic UnsafeCell-based Chain implementation
//!
//! This module provides a unified storage implementation that eliminates code
//! duplication between different storage types by providing a single, simple
//! generic chain implementation.

use std::cell::UnsafeCell;
use std::marker::PhantomData;

// ============================================================================
// Core Types
// ============================================================================

/// Terminal type for any chain
#[derive(Debug, Default, Clone)]
pub struct EmptyChain;

/// Generic chain structure with UnsafeCell-wrapped elements
#[derive(Debug)]
pub struct Chain<Head, Tail> {
    /// The head value wrapped in UnsafeCell for interior mutability
    head: UnsafeCell<Head>,
    /// The rest of the chain
    tail: Tail,
}

// ============================================================================
// Index Types
// ============================================================================

/// Index marker for the first element
pub struct Here;

/// Index marker for elements in the tail
pub struct There<I>(PhantomData<I>);

// ============================================================================
// Contains Trait for Type Existence Checking
// ============================================================================

/// Marker trait indicating that a chain contains a specific type
pub trait Contains<T> {}

// EmptyChain contains nothing (no implementations)

/// A chain contains its head type
impl<Head, Tail> Contains<Head> for Chain<Head, Tail> {}

// ============================================================================
// Runtime Type Checking
// ============================================================================

/// Runtime trait for checking if a chain contains a specific type
pub trait RuntimeContains {
    /// Check if a type exists in the chain at runtime using TypeId comparison
    fn contains<T: 'static>(&self) -> bool;
}

impl RuntimeContains for EmptyChain {
    fn contains<T: 'static>(&self) -> bool {
        false
    }
}

impl<Head: 'static, Tail: RuntimeContains> RuntimeContains for Chain<Head, Tail> {
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
impl<T, Tail> Selector<T, Here> for Chain<T, Tail> {
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
impl<Head, Tail, FromTail, TailIndex> Selector<FromTail, There<TailIndex>> for Chain<Head, Tail>
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
// Builder Trait
// ============================================================================

/// Generic trait for types that can have items added to them
pub trait ChainBuilder<T> {
    /// The type after adding an item
    type Output;

    /// Add an item with runtime duplicate checking
    fn with_item(self, item: T) -> Self::Output;
}

// Implement ChainBuilder for EmptyChain
impl<T> ChainBuilder<T> for EmptyChain {
    type Output = Chain<T, EmptyChain>;

    fn with_item(self, item: T) -> Self::Output {
        Chain {
            head: UnsafeCell::new(item),
            tail: self,
        }
    }
}

// Implement ChainBuilder for Chain
impl<T: 'static, Head, Tail> ChainBuilder<T> for Chain<Head, Tail>
where
    Self: RuntimeContains,
{
    type Output = Chain<T, Self>;

    fn with_item(self, item: T) -> Self::Output {
        assert!(
            !self.contains::<T>(),
            "Duplicate type in chain: {}",
            std::any::type_name::<T>()
        );
        Chain {
            head: UnsafeCell::new(item),
            tail: self,
        }
    }
}

impl EmptyChain {
    /// Create a new empty chain
    pub fn new() -> Self {
        Self
    }

    /// Start a new chain with a single value
    pub fn with_item<T>(self, item: T) -> Chain<T, EmptyChain> {
        ChainBuilder::with_item(self, item)
    }
}

impl<Head> Chain<Head, EmptyChain> {
    /// Create a new Chain with a single value
    pub fn new(head: Head) -> Self {
        Self {
            head: UnsafeCell::new(head),
            tail: EmptyChain,
        }
    }
}

// ============================================================================
// Convenience Methods
// ============================================================================

impl<Head, Tail> Chain<Head, Tail> {
    /// Get an immutable reference to a value from the chain by type
    pub fn get<T, Index>(&self) -> &T
    where
        Self: Selector<T, Index>,
    {
        Selector::get(self)
    }

    /// Get a mutable reference to a value from the chain by type
    #[allow(clippy::mut_from_ref)]
    pub fn get_mut<T, Index>(&self) -> &mut T
    where
        Self: Selector<T, Index>,
    {
        Selector::get_mut(self)
    }
}

// ============================================================================
// Safety Traits
// ============================================================================

// Safety: Chain is safe to send across threads because:
// 1. Each type appears exactly once
// 2. UnsafeCell provides thread-safe interior mutability
// 3. Items themselves must be Send
unsafe impl<Head: Send, Tail: Send> Send for Chain<Head, Tail> {}

// Safety: Chain is safe to share between threads because:
// 1. Interior mutability is controlled through UnsafeCell
// 2. Type system prevents aliasing of same type
// 3. Different types can be accessed concurrently
unsafe impl<Head: Sync, Tail: Sync> Sync for Chain<Head, Tail> {}

unsafe impl Send for EmptyChain {}
unsafe impl Sync for EmptyChain {}

// ============================================================================
// Clone Implementation
// ============================================================================

impl<Head: Clone, Tail: Clone> Clone for Chain<Head, Tail> {
    fn clone(&self) -> Self {
        Self {
            head: UnsafeCell::new(self.get().clone()),
            tail: self.tail.clone(),
        }
    }
}