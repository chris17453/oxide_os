//! Collection types for OXIDE OS
//!
//! Provides std::collections-like APIs using alloc.
//!
//! Note: HashMap and HashSet are not available in no_std without hashbrown.
//! Use BTreeMap and BTreeSet instead, which have O(log n) operations.

// Re-export from alloc::collections
pub use alloc::collections::{BTreeMap, BTreeSet, BinaryHeap, LinkedList, VecDeque};

// Re-export Vec and String as they're often used with collections
pub use alloc::string::String;
pub use alloc::vec::Vec;

// Type aliases for compatibility (use BTree* as fallback)
// These have O(log n) instead of O(1) for lookups, but work in no_std
pub type HashMap<K, V> = BTreeMap<K, V>;
pub type HashSet<T> = BTreeSet<T>;
