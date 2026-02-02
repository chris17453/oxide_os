//! Slab cache implementation
//!
//! A slab cache manages allocations of a single size class. Each slab is
//! typically one page (4KB) containing multiple objects of the same size.

use crate::size_class_index;
use core::alloc::Layout;
use core::ptr::NonNull;
use mm_core::FRAME_SIZE;
use spin::Mutex;

/// Physical memory map base for direct access
const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// A slab cache for allocating objects of a fixed size
pub struct SlabCache {
    /// Name of this cache (for debugging)
    name: &'static str,
    /// Size of each object in bytes
    obj_size: usize,
    /// Number of objects that fit in one slab (page)
    objs_per_slab: usize,
    /// List of slabs with some free objects
    partial: Mutex<SlabList>,
    /// List of slabs with all objects free (cached for reuse)
    empty: Mutex<SlabList>,
    /// Statistics
    allocated: Mutex<usize>,
    freed: Mutex<usize>,
}

/// A linked list of slabs
#[derive(Default)]
pub struct SlabList {
    head: Option<NonNull<SlabHeader>>,
    count: usize,
}

/// Header at the start of each slab page
#[repr(C)]
struct SlabHeader {
    /// Link to next slab in list
    next: Option<NonNull<SlabHeader>>,
    /// Number of free objects in this slab
    free_count: u16,
    /// Total objects in this slab
    total_count: u16,
    /// Head of free object list (index into objects)
    free_head: u16,
    /// Reserved for alignment
    _reserved: u16,
}

/// Free object node (stored in the object space itself)
#[repr(C)]
struct FreeObject {
    next: u16, // Index of next free object, or 0xFFFF if none
}

impl SlabCache {
    /// Create a new slab cache
    ///
    /// # Arguments
    /// * `name` - Name for debugging
    /// * `obj_size` - Size of objects to allocate
    pub const fn new(name: &'static str, obj_size: usize) -> Self {
        // Calculate actual object size (must fit at least a free node)
        let actual_size = if obj_size < core::mem::size_of::<FreeObject>() {
            core::mem::size_of::<FreeObject>()
        } else {
            obj_size
        };

        // Calculate objects per slab (accounting for header)
        let header_size = core::mem::size_of::<SlabHeader>();
        let usable_space = FRAME_SIZE - header_size;
        let objs_per_slab = usable_space / actual_size;

        Self {
            name,
            obj_size: actual_size,
            objs_per_slab,
            partial: Mutex::new(SlabList::new()),
            empty: Mutex::new(SlabList::new()),
            allocated: Mutex::new(0),
            freed: Mutex::new(0),
        }
    }

    /// Allocate an object from this cache
    ///
    /// # Safety
    /// Requires a frame allocator to be available for allocating new slabs.
    pub unsafe fn alloc(&self, alloc_page: impl FnOnce() -> Option<u64>) -> Option<NonNull<u8>> {
        // First, try to allocate from a partial slab
        {
            let mut partial = self.partial.lock();
            if let Some(ptr) = self.alloc_from_list(&mut partial) {
                *self.allocated.lock() += 1;
                return Some(ptr);
            }
        }

        // Try to use an empty (cached) slab
        {
            let mut empty = self.empty.lock();
            if let Some(slab) = empty.pop() {
                // Move slab to partial list
                let ptr = self.alloc_from_slab(slab);
                let mut partial = self.partial.lock();
                partial.push(slab);
                *self.allocated.lock() += 1;
                return ptr;
            }
        }

        // Need to allocate a new slab
        let page_phys = alloc_page()?;
        let page_virt = (PHYS_MAP_BASE + page_phys) as *mut u8;
        let slab = NonNull::new(page_virt as *mut SlabHeader)?;

        // Initialize the new slab
        // SAFETY: page is freshly allocated and valid
        unsafe { self.init_slab(slab) };

        // Allocate from the new slab
        let ptr = self.alloc_from_slab(slab);

        // Add slab to partial list
        let mut partial = self.partial.lock();
        partial.push(slab);

        *self.allocated.lock() += 1;
        ptr
    }

    /// Free an object back to this cache
    ///
    /// # Safety
    /// The pointer must have been allocated from this cache.
    pub unsafe fn free(&self, ptr: NonNull<u8>, free_page: impl FnOnce(u64)) {
        // Find the slab this object belongs to (page-aligned address)
        let ptr_addr = ptr.as_ptr() as usize;
        let slab_addr = ptr_addr & !(FRAME_SIZE - 1);
        // SAFETY: slab_addr is page-aligned and points to a valid slab header
        let slab = unsafe { NonNull::new_unchecked(slab_addr as *mut SlabHeader) };

        // Calculate object index
        let header_size = core::mem::size_of::<SlabHeader>();
        let obj_offset = ptr_addr - slab_addr - header_size;
        let obj_index = obj_offset / self.obj_size;

        // Add object to slab's free list
        // SAFETY: Caller guarantees ptr was allocated from this cache
        let slab_mut = unsafe { &mut *slab.as_ptr() };

        // Link object into free list
        let obj_ptr = ptr.as_ptr() as *mut FreeObject;
        unsafe {
            (*obj_ptr).next = slab_mut.free_head;
        }
        slab_mut.free_head = obj_index as u16;
        slab_mut.free_count += 1;

        *self.freed.lock() += 1;

        // Check if slab is now completely empty
        if slab_mut.free_count == slab_mut.total_count {
            // Remove from partial list and add to empty list
            let mut partial = self.partial.lock();
            if partial.remove(slab) {
                let mut empty = self.empty.lock();
                // Limit cached empty slabs
                if empty.count < 4 {
                    empty.push(slab);
                } else {
                    // Free the page back to the frame allocator
                    let page_phys = slab_addr as u64 - PHYS_MAP_BASE;
                    drop(empty);
                    drop(partial);
                    free_page(page_phys);
                }
            }
        }
    }

    /// Initialize a new slab
    ///
    /// # Safety
    /// The slab pointer must point to a valid, freshly allocated page.
    unsafe fn init_slab(&self, slab: NonNull<SlabHeader>) {
        let header = slab.as_ptr();
        let header_size = core::mem::size_of::<SlabHeader>();
        let obj_base = (header as usize + header_size) as *mut u8;

        // Initialize header
        unsafe {
            (*header).next = None;
            (*header).free_count = self.objs_per_slab as u16;
            (*header).total_count = self.objs_per_slab as u16;
            (*header).free_head = 0;
            (*header)._reserved = 0;
        }

        // Initialize free list (link all objects together)
        for i in 0..self.objs_per_slab {
            let obj = unsafe { obj_base.add(i * self.obj_size) as *mut FreeObject };
            unsafe {
                (*obj).next = if i + 1 < self.objs_per_slab {
                    (i + 1) as u16
                } else {
                    0xFFFF
                };
            }
        }
    }

    /// Allocate from a specific slab
    fn alloc_from_slab(&self, slab: NonNull<SlabHeader>) -> Option<NonNull<u8>> {
        let header_size = core::mem::size_of::<SlabHeader>();

        // SAFETY: slab is valid
        let header = unsafe { &mut *slab.as_ptr() };
        if header.free_count == 0 {
            return None;
        }

        let obj_index = header.free_head;
        let obj_base = (slab.as_ptr() as usize + header_size) as *mut u8;
        let obj_ptr = unsafe { obj_base.add(obj_index as usize * self.obj_size) };

        // Update free list
        let free_obj = obj_ptr as *const FreeObject;
        header.free_head = unsafe { (*free_obj).next };
        header.free_count -= 1;

        NonNull::new(obj_ptr)
    }

    /// Try to allocate from a slab list
    fn alloc_from_list(&self, list: &mut SlabList) -> Option<NonNull<u8>> {
        let mut current = list.head;
        while let Some(slab) = current {
            if let Some(ptr) = self.alloc_from_slab(slab) {
                // Check if slab is now full
                let header = unsafe { &*slab.as_ptr() };
                if header.free_count == 0 {
                    // Remove from partial list (slab is full now)
                    list.remove(slab);
                }
                return Some(ptr);
            }
            current = unsafe { (*slab.as_ptr()).next };
        }
        None
    }

    /// Get the object size for this cache
    pub fn obj_size(&self) -> usize {
        self.obj_size
    }

    /// Get the cache name
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Get allocation statistics
    pub fn stats(&self) -> (usize, usize) {
        (*self.allocated.lock(), *self.freed.lock())
    }
}

impl SlabList {
    /// Create a new empty slab list
    pub const fn new() -> Self {
        Self {
            head: None,
            count: 0,
        }
    }

    /// Push a slab to the front of the list
    fn push(&mut self, slab: NonNull<SlabHeader>) {
        unsafe {
            (*slab.as_ptr()).next = self.head;
        }
        self.head = Some(slab);
        self.count += 1;
    }

    /// Pop a slab from the front of the list
    fn pop(&mut self) -> Option<NonNull<SlabHeader>> {
        let slab = self.head?;
        self.head = unsafe { (*slab.as_ptr()).next };
        self.count -= 1;
        Some(slab)
    }

    /// Remove a specific slab from the list
    fn remove(&mut self, target: NonNull<SlabHeader>) -> bool {
        // Check if target is the head
        if self.head == Some(target) {
            self.head = unsafe { (*target.as_ptr()).next };
            self.count -= 1;
            return true;
        }

        // Search through the list
        let mut current = self.head;
        while let Some(slab) = current {
            let next = unsafe { (*slab.as_ptr()).next };
            if next == Some(target) {
                // Found it - unlink
                unsafe {
                    (*slab.as_ptr()).next = (*target.as_ptr()).next;
                }
                self.count -= 1;
                return true;
            }
            current = next;
        }

        false
    }
}

/// Manager for multiple size-class slab caches
pub struct SlabCacheManager {
    /// Size-class caches (8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096)
    caches: [SlabCache; 10],
}

impl SlabCacheManager {
    /// Create a new slab cache manager with size-class caches
    pub const fn new() -> Self {
        Self {
            caches: [
                SlabCache::new("kmalloc-8", 8),
                SlabCache::new("kmalloc-16", 16),
                SlabCache::new("kmalloc-32", 32),
                SlabCache::new("kmalloc-64", 64),
                SlabCache::new("kmalloc-128", 128),
                SlabCache::new("kmalloc-256", 256),
                SlabCache::new("kmalloc-512", 512),
                SlabCache::new("kmalloc-1024", 1024),
                SlabCache::new("kmalloc-2048", 2048),
                SlabCache::new("kmalloc-4096", 4096),
            ],
        }
    }

    /// Allocate memory of the given layout
    ///
    /// # Safety
    /// Requires a frame allocator for new slabs.
    pub unsafe fn alloc(
        &self,
        layout: Layout,
        alloc_page: impl FnOnce() -> Option<u64>,
    ) -> Option<NonNull<u8>> {
        let size = layout.size().max(layout.align());
        let idx = size_class_index(size)?;
        // SAFETY: Caller provides valid alloc_page
        unsafe { self.caches[idx].alloc(alloc_page) }
    }

    /// Free memory
    ///
    /// # Safety
    /// Pointer must have been allocated from this manager.
    pub unsafe fn free(&self, ptr: NonNull<u8>, layout: Layout, free_page: impl FnOnce(u64)) {
        let size = layout.size().max(layout.align());
        if let Some(idx) = size_class_index(size) {
            // SAFETY: Caller guarantees ptr was allocated from this manager
            unsafe { self.caches[idx].free(ptr, free_page) };
        }
    }

    /// Get the cache for a specific size class index
    pub fn cache(&self, idx: usize) -> Option<&SlabCache> {
        self.caches.get(idx)
    }

    /// Get all caches
    pub fn caches(&self) -> &[SlabCache] {
        &self.caches
    }
}

impl Default for SlabCacheManager {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: SlabCacheManager uses internal locking
unsafe impl Send for SlabCache {}
unsafe impl Sync for SlabCache {}
unsafe impl Send for SlabCacheManager {}
unsafe impl Sync for SlabCacheManager {}
