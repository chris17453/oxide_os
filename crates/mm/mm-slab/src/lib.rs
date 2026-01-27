//! OXIDE Slab Allocator
//!
//! Slab allocation for fixed-size kernel objects. Provides efficient allocation
//! and deallocation of frequently used object types with minimal fragmentation.
//!
//! Features:
//! - Size-class based caches (8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096)
//! - Per-CPU free lists (future)
//! - Object coloring for cache efficiency (future)
//! - SLAB/SLUB style partial list management

#![no_std]

mod cache;

pub use cache::{SlabCache, SlabCacheManager};

use core::alloc::Layout;
use core::ptr::NonNull;
use mm_core::{MmError, MmResult, FRAME_SIZE};

/// Minimum slab object size (must fit a free list pointer)
pub const MIN_SLAB_SIZE: usize = 8;

/// Maximum slab object size for slab allocator
/// Objects larger than this should use the page allocator directly
pub const MAX_SLAB_SIZE: usize = FRAME_SIZE / 2;

/// Slab size classes (object sizes in bytes)
pub const SIZE_CLASSES: [usize; 10] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];

/// Find the appropriate size class index for an allocation
pub fn size_class_index(size: usize) -> Option<usize> {
    for (i, &class_size) in SIZE_CLASSES.iter().enumerate() {
        if size <= class_size {
            return Some(i);
        }
    }
    None
}

/// Get the size class for a given size
pub fn size_class_for(size: usize) -> Option<usize> {
    for &class_size in &SIZE_CLASSES {
        if size <= class_size {
            return Some(class_size);
        }
    }
    None
}

/// Allocate from a size class
///
/// This is a convenience function that uses the global slab cache manager
/// if available. For direct control, use SlabCacheManager directly.
pub fn slab_alloc(layout: Layout) -> MmResult<NonNull<u8>> {
    // For sizes larger than max slab size, fail
    if layout.size() > MAX_SLAB_SIZE {
        return Err(MmError::InvalidOrder);
    }

    // This would use a global slab cache manager
    // For now, return an error since we haven't set up the global
    Err(MmError::OutOfMemory)
}

/// Free to a size class
pub fn slab_free(_ptr: NonNull<u8>, layout: Layout) -> MmResult<()> {
    if layout.size() > MAX_SLAB_SIZE {
        return Err(MmError::InvalidOrder);
    }

    // This would use a global slab cache manager
    Err(MmError::InvalidAddress)
}
