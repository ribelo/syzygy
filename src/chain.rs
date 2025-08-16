//! Generic compile-time chain system for zero-overhead type-safe access
//!
//! This module provides a type-level linked list that enables compile-time
//! verification and zero-cost access to multiple values. Unlike HashMap-based
//! storage, chain access is resolved at compile time with direct memory offsets.
//!
//! This generic chain can be used for both models and resources:
//! - Models: Stored directly, mutable access in CommandContext
//! - Resources: Stored as Arc<T>, immutable access in EffectContext
//!
//! # Usage Pattern
//!
//! The recommended way to access chain items is using type annotations on the left:
//!
//! ```rust,ignore
//! let user: &UserModel = chain.get();        // Clean!
//! let db: &Arc<Database> = chain.get();      // Beautiful!
//! let mut post: &mut PostModel = chain.get_mut(); // Mutable!
//! ```
//!
//! Uses index types (Here/There) to disambiguate trait implementations,
//! exactly like frunk's HList but tailored for Syzygy's needs.

use std::marker::PhantomData;

// ============================================================================
// Core Types
// ============================================================================

/// A type-level chain of values
///
/// Each value is stored directly in the chain with its type preserved.
/// The compiler resolves access patterns at compile time, eliminating
/// all runtime overhead.
#[derive(Debug, Clone)]
pub struct Chain<Head, Tail> {
    /// The head value in the chain
    pub head: Head,
    /// The tail of the chain (either another Chain or NoChain)
    pub tail: Tail,
}

/// Terminal type for chains
///
/// This represents the end of a chain. If you try to access a value
/// that doesn't exist in the chain, the code simply won't compile.
#[derive(Debug, Clone, Default)]
pub struct NoChain;

// ============================================================================
// Index Types for Disambiguation (exactly like frunk)
// ============================================================================

/// Index type indicating the target is at the head of the chain
pub struct Here;

/// Index type indicating the target is somewhere in the tail
pub struct There<Index> {
    _phantom: PhantomData<Index>,
}

// ============================================================================
// Selector Trait - Core Implementation (exactly like frunk)
// ============================================================================

/// Low-level trait for selecting a value from a chain using index types
///
/// This trait uses index types to avoid overlapping implementations.
pub trait Selector<T, Index> {
    /// Get an immutable reference to the value
    fn get(&self) -> &T;

    /// Get a mutable reference to the value
    fn get_mut(&mut self) -> &mut T;
}

// Implementation when T is at the head (Here index)
impl<T, Tail> Selector<T, Here> for Chain<T, Tail> {
    #[inline(always)]
    fn get(&self) -> &T {
        &self.head
    }

    #[inline(always)]
    fn get_mut(&mut self) -> &mut T {
        &mut self.head
    }
}

// Implementation when T is in the tail (There index)
impl<Head, Tail, FromTail, TailIndex> Selector<FromTail, There<TailIndex>> for Chain<Head, Tail>
where
    Tail: Selector<FromTail, TailIndex>,
{
    #[inline(always)]
    fn get(&self) -> &FromTail {
        self.tail.get()
    }

    #[inline(always)]
    fn get_mut(&mut self) -> &mut FromTail {
        self.tail.get_mut()
    }
}

// ============================================================================
// Inherent Methods on Chain
// ============================================================================

impl<Head> Chain<Head, NoChain> {
    /// Create a new chain with a single value
    pub fn new(head: Head) -> Self {
        Chain {
            head,
            tail: NoChain,
        }
    }
}

impl<Head, Tail> Chain<Head, Tail> {
    /// Get a value from the chain by type
    ///
    /// The cleanest way to use this is with type annotation on the left:
    ///
    /// # Examples
    /// ```rust,ignore
    /// // RECOMMENDED: Type annotation on the left side
    /// let user: &UserModel = chain.get();
    /// let db: &Arc<Database> = chain.get();
    ///
    /// // Alternative: Turbofish with underscore for index
    /// let user = chain.get::<UserModel, _>();
    /// ```
    ///
    /// The index type parameter is automatically inferred by the compiler.
    #[inline(always)]
    pub fn get<T, Index>(&self) -> &T
    where
        Self: Selector<T, Index>,
    {
        Selector::get(self)
    }

    /// Get a mutable reference to a value by type
    ///
    /// # Examples
    /// ```rust,ignore
    /// // RECOMMENDED: Type annotation on the left side
    /// let user: &mut UserModel = chain.get_mut();
    ///
    /// // Alternative: Turbofish with underscore
    /// let user = chain.get_mut::<UserModel, _>();
    /// ```
    #[inline(always)]
    pub fn get_mut<T, Index>(&mut self) -> &mut T
    where
        Self: Selector<T, Index>,
    {
        Selector::get_mut(self)
    }

    /// Add a value to the front of the chain
    pub fn push<V>(self, value: V) -> Chain<V, Self> {
        Chain {
            head: value,
            tail: self,
        }
    }

    /// Get a reference to the head value
    pub fn head(&self) -> &Head {
        &self.head
    }

    /// Get a mutable reference to the head value
    pub fn head_mut(&mut self) -> &mut Head {
        &mut self.head
    }

    /// Get a reference to the tail
    pub fn tail(&self) -> &Tail {
        &self.tail
    }

    /// Get a mutable reference to the tail
    pub fn tail_mut(&mut self) -> &mut Tail {
        &mut self.tail
    }
}

// ============================================================================
// Builder Pattern Support
// ============================================================================

/// Helper trait for building chains
pub trait ChainBuilder {
    /// Add a value to the chain
    fn with<V>(self, value: V) -> Chain<V, Self>
    where
        Self: Sized,
    {
        Chain {
            head: value,
            tail: self,
        }
    }
}

// Any existing chain can be extended
impl<Head, Tail> ChainBuilder for Chain<Head, Tail> {}

// NoChain can start a chain
impl ChainBuilder for NoChain {}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Debug, PartialEq, Clone)]
    struct UserModel {
        name: String,
    }

    #[derive(Debug, PartialEq, Clone)]
    struct PostModel {
        title: String,
    }

    #[derive(Debug, PartialEq)]
    struct Database {
        connected: bool,
    }

    #[test]
    fn test_single_value_chain() {
        let mut chain = Chain::new(UserModel {
            name: "Alice".to_string()
        });

        // RECOMMENDED: Clean syntax with type annotation on the left
        let user: &UserModel = chain.get();
        assert_eq!(user.name, "Alice");

        // Test mutable access with type annotation
        let user_mut: &mut UserModel = chain.get_mut();
        user_mut.name = "Bob".to_string();

        // Verify change
        let user: &UserModel = chain.get();
        assert_eq!(user.name, "Bob");
    }

    #[test]
    fn test_multi_value_chain() {
        let mut chain = Chain {
            head: PostModel { title: "Hello".to_string() },
            tail: Chain {
                head: UserModel { name: "Alice".to_string() },
                tail: Chain {
                    head: Arc::new(Database { connected: true }),
                    tail: NoChain,
                }
            }
        };

        // BEAUTIFUL: Clean syntax with type annotations on the left!
        let post: &PostModel = chain.get();
        assert_eq!(post.title, "Hello");

        let user: &UserModel = chain.get();
        assert_eq!(user.name, "Alice");

        let db: &Arc<Database> = chain.get();
        assert!(db.connected);

        // Test mutable access with clean type annotations
        {
            let post_mut: &mut PostModel = chain.get_mut();
            post_mut.title = "World".to_string();
        }

        {
            let user_mut: &mut UserModel = chain.get_mut();
            user_mut.name = "Bob".to_string();
        }

        // Arc resources can be accessed but not mutated through the chain
        // (the Arc itself is immutable, but you could clone and modify the contents)

        // Verify changes - still using clean type annotations
        let post: &PostModel = chain.get();
        assert_eq!(post.title, "World");

        let user: &UserModel = chain.get();
        assert_eq!(user.name, "Bob");
    }

    #[test]
    fn test_chain_builder() {
        let chain = NoChain
            .with(UserModel { name: "Alice".to_string() })
            .with(PostModel { title: "Hello".to_string() })
            .with(Arc::new(Database { connected: true }));

        // BEAUTIFUL: Clean syntax with type annotations!
        let db: &Arc<Database> = chain.get();
        assert!(db.connected);

        let post: &PostModel = chain.get();
        assert_eq!(post.title, "Hello");

        let user: &UserModel = chain.get();
        assert_eq!(user.name, "Alice");

        // All resolved at compile time - zero runtime overhead!
    }

    #[test]
    fn test_push_method() {
        let chain = Chain::new(UserModel { name: "Alice".to_string() })
            .push(PostModel { title: "Hello".to_string() })
            .push(Arc::new(Database { connected: true }));

        // The chain is built in reverse order with push
        let db = chain.get::<Arc<Database>, _>();
        assert!(db.connected);

        let post = chain.get::<PostModel, _>();
        assert_eq!(post.title, "Hello");

        let user = chain.get::<UserModel, _>();
        assert_eq!(user.name, "Alice");
    }

    #[test]
    fn test_turbofish_syntax() {
        let chain = NoChain
            .with(UserModel { name: "Alice".to_string() })
            .with(PostModel { title: "Hello".to_string() })
            .with(Arc::new(Database { connected: true }));

        // Turbofish syntax with index inference
        assert!(chain.get::<Arc<Database>, _>().connected);
        assert_eq!(chain.get::<PostModel, _>().title, "Hello");
        assert_eq!(chain.get::<UserModel, _>().name, "Alice");
    }

    #[test]
    fn test_mixed_types() {
        // Test chain with both owned values and Arc-wrapped values
        let mut chain = NoChain
            .with(String::from("config"))              // Owned
            .with(Arc::new(Database { connected: true }))   // Arc
            .with(42u32);                              // Owned

        let config: &String = chain.get();
        assert_eq!(config, "config");

        let db: &Arc<Database> = chain.get();
        assert!(db.connected);

        let number: &mut u32 = chain.get_mut();
        *number = 100;

        let number: &u32 = chain.get();
        assert_eq!(*number, 100);
    }

    // This test would fail to compile if uncommented, which is exactly what we want!
    // #[test]
    // fn test_compile_time_safety() {
    //     struct NotInChain;
    //     let chain = Chain::new(UserModel { name: "Alice".to_string() });
    //     let not_found: &NotInChain = chain.get::<NotInChain, _>(); // Compile error!
    // }
}
