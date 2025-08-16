//! IndexedMap - Generic zero-overhead indexed storage
//!
//! This module provides a generic, compile-time indexed map that can store
//! any type of value using array-based storage with constant-time access.
//! Used as the foundation for both EventMap and ModelMap.

use std::marker::PhantomData;
use std::mem::MaybeUninit;

/// Trait for array types used by IndexedMap
///
/// This trait provides array operations needed by IndexedMap.
/// Safety: LENGTH must match the actual array size.
pub unsafe trait IndexArray<V>: Sized {
    /// The actual length of the array
    const LENGTH: usize;

    /// Get a slice view of the array
    fn as_slice(&self) -> &[V];

    /// Get a mutable slice view of the array
    fn as_mut_slice(&mut self) -> &mut [V];
}

// Implement IndexArray for standard arrays
unsafe impl<V, const N: usize> IndexArray<V> for [V; N] {
    const LENGTH: usize = N;

    #[inline(always)]
    fn as_slice(&self) -> &[V] {
        self
    }

    #[inline(always)]
    fn as_mut_slice(&mut self) -> &mut [V] {
        self
    }
}

/// Trait for types that can be used as compile-time indices
///
/// This trait enables types to be used as array indices for
/// zero-overhead access. Each indexable type defines its length
/// and can convert instances to array indices.
pub trait Indexable: Sized {
    /// Number of possible indices
    const LENGTH: usize;

    /// Array type for storing values indexed by this type
    type Array<V>: IndexArray<V>;

    /// Convert this instance to its array index
    fn index(&self) -> usize;
}

/// Generic zero-overhead indexed map
///
/// This struct stores values in a fixed-size array indexed by compile-time
/// constants. It provides constant-time access with zero runtime overhead.
///
/// # Type Parameters
///
/// - `I`: The indexable type (enum variants, model types, etc.)
/// - `V`: The value type to store (function pointers, model pointers, etc.)
///
/// # Safety
///
/// This struct maintains these invariants:
/// 1. All array indices are within bounds (enforced by I::LENGTH)
/// 2. Array is properly initialized before use
/// 3. Values at each index have the expected type for that index
pub struct IndexedMap<I: Indexable, V> {
    /// Array storage for values
    storage: I::Array<V>,
    _phantom: PhantomData<I>,
}

impl<I: Indexable, V> IndexedMap<I, V> {
    /// Create a new IndexedMap from a pre-initialized storage array
    ///
    /// This is the fundamental constructor that takes ownership of a storage array.
    /// Other constructors should build on top of this one.
    pub const fn new(storage: I::Array<V>) -> Self {
        Self {
            storage,
            _phantom: PhantomData,
        }
    }

    /// Create a new IndexedMap with all values initialized to the provided default
    pub fn new_with_default(default_value: V) -> Self 
    where
        V: Copy,
    {
        // Initialize the array with the default value using MaybeUninit pattern
        let mut array: MaybeUninit<I::Array<V>> = MaybeUninit::uninit();
        let array_ptr = array.as_mut_ptr() as *mut V;

        // Initialize each element to the default value
        for i in 0..I::LENGTH {
            unsafe {
                array_ptr.add(i).write(default_value);
            }
        }

        // Safety: We've initialized all elements
        let storage = unsafe { array.assume_init() };

        Self {
            storage,
            _phantom: PhantomData,
        }
    }

    /// Set a value at the specified index
    ///
    /// # Safety
    ///
    /// The caller must ensure that `index` is a valid index for type I
    /// and that the value type matches what's expected at that index.
    #[inline(always)]
    pub unsafe fn set(&mut self, index: usize, value: V) {
        debug_assert!(index < I::LENGTH, "Index out of bounds");
        self.storage.as_mut_slice()[index] = value;
    }

    /// Get a reference to the value at the specified index
    ///
    /// # Safety
    ///
    /// The caller must ensure that `index` is a valid index for type I
    /// and that the value at that index has been properly initialized.
    #[inline(always)]
    pub unsafe fn get(&self, index: usize) -> &V {
        debug_assert!(index < I::LENGTH, "Index out of bounds");
        &self.storage.as_slice()[index]
    }

    /// Get a mutable reference to the value at the specified index
    ///
    /// # Safety
    ///
    /// The caller must ensure that `index` is a valid index for type I
    /// and that the value at that index has been properly initialized.
    #[inline(always)]
    pub unsafe fn get_mut(&mut self, index: usize) -> &mut V {
        debug_assert!(index < I::LENGTH, "Index out of bounds");
        &mut self.storage.as_mut_slice()[index]
    }

    /// Get a reference to the value for the given indexable instance
    ///
    /// This is a safe wrapper around get() that uses the indexable's
    /// own index() method.
    #[inline(always)]
    pub fn get_for(&self, indexable: &I) -> &V {
        let index = indexable.index();
        debug_assert!(index < I::LENGTH, "Invalid index from indexable");
        unsafe { self.get(index) }
    }

    /// Get a mutable reference to the value for the given indexable instance
    ///
    /// This is a safe wrapper around get_mut() that uses the indexable's
    /// own index() method.
    #[inline(always)]
    pub fn get_mut_for(&mut self, indexable: &I) -> &mut V {
        let index = indexable.index();
        debug_assert!(index < I::LENGTH, "Invalid index from indexable");
        unsafe { self.get_mut(index) }
    }

    /// Get the underlying storage as a slice
    #[inline]
    pub fn as_slice(&self) -> &[V] {
        self.storage.as_slice()
    }

    /// Get the underlying storage as a mutable slice
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [V] {
        self.storage.as_mut_slice()
    }

    /// Get the length of the storage
    #[inline]
    pub const fn len(&self) -> usize {
        I::LENGTH
    }

    /// Check if the storage is empty (always false for fixed-size arrays)
    #[inline]
    pub const fn is_empty(&self) -> bool {
        I::LENGTH == 0
    }
}

impl<I: Indexable, V: Default + Copy> Default for IndexedMap<I, V> {
    fn default() -> Self {
        Self::new_with_default(V::default())
    }
}

// Specialized implementations for pointer types (common use case)
impl<I: Indexable> IndexedMap<I, *const ()> {
    /// Create a new IndexedMap with all pointers initialized to null
    ///
    /// This function can be evaluated at compile-time when the array size is known.
    pub fn new_null_pointers() -> Self {
        Self::new_with_default(std::ptr::null())
    }

    /// Check if the pointer at the given index is null
    #[inline]
    pub fn is_null_at(&self, index: usize) -> bool {
        debug_assert!(index < I::LENGTH, "Index out of bounds");
        unsafe { self.get(index).is_null() }
    }

    /// Count the number of non-null pointers
    #[inline]
    pub fn non_null_count(&self) -> usize {
        self.as_slice().iter().filter(|ptr| !ptr.is_null()).count()
    }
}

impl<I: Indexable> IndexedMap<I, *mut ()> {
    /// Create a new IndexedMap with all pointers initialized to null
    pub fn new_null_mut_pointers() -> Self {
        Self::new_with_default(std::ptr::null_mut())
    }

    /// Check if the pointer at the given index is null
    #[inline]
    pub fn is_null_at(&self, index: usize) -> bool {
        debug_assert!(index < I::LENGTH, "Index out of bounds");
        unsafe { self.get(index).is_null() }
    }

    /// Count the number of non-null pointers
    #[inline]
    pub fn non_null_count(&self) -> usize {
        self.as_slice().iter().filter(|ptr| !ptr.is_null()).count()
    }
}

// Specialized implementation for *mut dyn Any (common for model storage)
impl<I: Indexable> IndexedMap<I, *mut dyn std::any::Any> {
    /// Create a new IndexedMap with all pointers initialized to null
    pub fn new_null_mut_any_pointers() -> Self {
        // For fat pointers like *mut dyn Any, we need to create a proper null pointer
        let null_ptr: *mut dyn std::any::Any = std::ptr::null_mut::<()>() as *mut dyn std::any::Any;
        Self::new_with_default(null_ptr)
    }

    /// Check if the pointer at the given index is null
    #[inline]
    pub fn is_null_at(&self, index: usize) -> bool {
        debug_assert!(index < I::LENGTH, "Index out of bounds");
        unsafe { 
            let ptr = *self.get(index);
            // For fat pointers, we convert to thin pointer to check nullness
            let thin_ptr = ptr as *mut u8;
            thin_ptr.is_null()
        }
    }

    /// Count the number of non-null pointers
    #[inline]
    pub fn non_null_count(&self) -> usize {
        self.as_slice().iter().filter(|ptr| {
            // For fat pointers, we convert to thin pointer to check nullness
            let thin_ptr = **ptr as *mut u8;
            !thin_ptr.is_null()
        }).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test enum for indexing
    #[derive(Debug, Clone)]
    enum TestIndex {
        First,
        Second,
        Third,
    }

    impl Indexable for TestIndex {
        const LENGTH: usize = 3;
        type Array<V> = [V; 3];

        fn index(&self) -> usize {
            match self {
                TestIndex::First => 0,
                TestIndex::Second => 1,
                TestIndex::Third => 2,
            }
        }
    }

    #[test]
    fn test_indexed_map_basic_operations() {
        let mut map = IndexedMap::<TestIndex, i32>::new_with_default(0);

        // Test setting and getting values
        unsafe {
            map.set(0, 10);
            map.set(1, 20);
            map.set(2, 30);

            assert_eq!(*map.get(0), 10);
            assert_eq!(*map.get(1), 20);
            assert_eq!(*map.get(2), 30);
        }

        // Test using indexable instances
        assert_eq!(*map.get_for(&TestIndex::First), 10);
        assert_eq!(*map.get_for(&TestIndex::Second), 20);
        assert_eq!(*map.get_for(&TestIndex::Third), 30);
    }

    #[test]
    fn test_indexed_map_mutable_operations() {
        let mut map = IndexedMap::<TestIndex, i32>::new_with_default(0);

        // Test mutable access
        *map.get_mut_for(&TestIndex::First) = 100;
        *map.get_mut_for(&TestIndex::Second) = 200;

        assert_eq!(*map.get_for(&TestIndex::First), 100);
        assert_eq!(*map.get_for(&TestIndex::Second), 200);
        assert_eq!(*map.get_for(&TestIndex::Third), 0);
    }

    #[test]
    fn test_indexed_map_pointer_operations() {
        let map = IndexedMap::<TestIndex, *const ()>::new_null_pointers();

        // All pointers should be null initially
        assert!(map.is_null_at(0));
        assert!(map.is_null_at(1));
        assert!(map.is_null_at(2));
        assert_eq!(map.non_null_count(), 0);

        // Test with a non-null pointer
        let mut map = map;
        let dummy = 42i32;
        unsafe {
            map.set(0, &dummy as *const i32 as *const ());
        }

        assert!(!map.is_null_at(0));
        assert!(map.is_null_at(1));
        assert!(map.is_null_at(2));
        assert_eq!(map.non_null_count(), 1);
    }

    #[test]
    fn test_indexed_map_slice_operations() {
        let mut map = IndexedMap::<TestIndex, i32>::new_with_default(0);

        // Set some values
        unsafe {
            map.set(0, 10);
            map.set(1, 20);
            map.set(2, 30);
        }

        // Test slice access
        let slice = map.as_slice();
        assert_eq!(slice.len(), 3);
        assert_eq!(slice[0], 10);
        assert_eq!(slice[1], 20);
        assert_eq!(slice[2], 30);

        // Test mutable slice access
        let mut_slice = map.as_mut_slice();
        mut_slice[1] = 25;
        assert_eq!(*map.get_for(&TestIndex::Second), 25);
    }

    #[test]
    fn test_indexed_map_metadata() {
        let map = IndexedMap::<TestIndex, i32>::new_with_default(42);

        assert_eq!(map.len(), 3);
        assert!(!map.is_empty());

        // Test with zero-length type (if such a thing existed)
        struct EmptyIndex;
        impl Indexable for EmptyIndex {
            const LENGTH: usize = 0;
            type Array<V> = [V; 0];
            fn index(&self) -> usize { 0 }
        }

        let empty_map = IndexedMap::<EmptyIndex, i32>::new_with_default(0);
        assert_eq!(empty_map.len(), 0);
        assert!(empty_map.is_empty());
    }
}