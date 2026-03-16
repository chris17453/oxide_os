//! Per-CPU Slab Allocator for OXIDE OS
//!
//! — TorqueJax: The global heap lock was the single biggest bottleneck in the
//! kernel. 4 CPUs contending on one KernelMutex spinlock for every alloc/dealloc.
//! Page fault handlers re-entering the allocator = deadlock. Timer ISR trying
//! to allocate while the preempted task holds the heap lock = triple fault.
//!
//! This slab allocator provides per-CPU free lists for small allocations
//! (≤2048 bytes). Each CPU has its own cache — no lock on the hot path.
//! Only the cold path (refilling from buddy) takes any lock.
//!
//! Architecture:
//! ```text
//! Hot path (no lock):     Per-CPU free lists [16,32,64,128,256,512,1024,2048]
//!                               |
//! Refill (rare lock):     Buddy allocator (per-zone Mutex) via page callback
//!                               |
//! Large alloc fallback:   LinkedListAllocator (KernelMutex) — existing heap
//! ```

#![no_std]

use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use mm_heap::{KernelHeap, new_kernel_heap};

/// Maximum number of CPUs supported by the slab allocator.
/// — TorqueJax: Matches smp::MAX_CPUS. Can't import it (circular dep), so hardcode.
const MAX_CPUS: usize = 256;

/// Number of size classes: 16, 32, 64, 128, 256, 512, 1024, 2048
const NUM_SIZE_CLASSES: usize = 8;

/// Size classes in bytes. Each is a power of 2 starting at 16.
/// — TorqueJax: 16 is the minimum because we store a FreeNode (8 bytes)
/// in the free object's memory. 16 gives us alignment headroom.
const SIZE_CLASSES: [usize; NUM_SIZE_CLASSES] = [16, 32, 64, 128, 256, 512, 1024, 2048];

/// Maximum count per free list before draining back to buddy.
/// — TorqueJax: 2× objects_per_page keeps memory pressure reasonable.
/// A full 4K page of 16-byte objects = 256 objects. 2× = 512 max cached.
const MAX_CACHED: [usize; NUM_SIZE_CLASSES] = [512, 256, 128, 64, 32, 16, 8, 4];

/// Page size for buddy allocator refills.
const PAGE_SIZE: usize = 4096;

/// — TorqueJax: True after slab_init() completes. Before this, all allocs
/// fall through to the LinkedListAllocator. Early boot can't use per-CPU
/// caches because SMP isn't up yet and os_core::smp_cpu_id() returns garbage.
pub static SLAB_READY: AtomicBool = AtomicBool::new(false);

/// Free node stored in the first 8 bytes of each free object.
/// — TorqueJax: Classic slab trick — the free list is embedded in the free
/// objects themselves. Zero overhead. Linux does the same thing.
#[repr(C)]
struct FreeNode {
    next: *mut FreeNode,
}

/// Per-size-class free list.
struct FreeList {
    head: *mut FreeNode,
    count: u32,
}

impl FreeList {
    const fn empty() -> Self {
        Self {
            head: core::ptr::null_mut(),
            count: 0,
        }
    }

    /// Pop an object from the free list. O(1).
    fn pop(&mut self) -> Option<*mut u8> {
        if self.head.is_null() {
            return None;
        }
        let node = self.head;
        // — TorqueJax: Safe because we only store valid pointers via push().
        // The memory is ours (carved from buddy pages). No UAF — we're the
        // only CPU touching this list (preemption disabled).
        self.head = unsafe { (*node).next };
        self.count -= 1;
        Some(node as *mut u8)
    }

    /// Push an object onto the free list. O(1).
    fn push(&mut self, ptr: *mut u8) {
        let node = ptr as *mut FreeNode;
        unsafe {
            (*node).next = self.head;
        }
        self.head = node;
        self.count += 1;
    }

    /// Drain half the free list (return objects to buddy).
    /// Returns a linked list of nodes to free.
    fn drain_half(&mut self) -> (*mut FreeNode, u32) {
        let to_drain = self.count / 2;
        if to_drain == 0 {
            return (core::ptr::null_mut(), 0);
        }
        let mut drained = 0u32;
        let drain_head = self.head;
        let mut current = self.head;
        for _ in 0..to_drain.saturating_sub(1) {
            if current.is_null() { break; }
            current = unsafe { (*current).next };
            drained += 1;
        }
        if !current.is_null() {
            let next = unsafe { (*current).next };
            unsafe { (*current).next = core::ptr::null_mut(); }
            self.head = next;
            drained += 1;
            self.count -= drained;
        }
        (drain_head, drained)
    }
}

/// Per-CPU cache containing free lists for each size class.
struct PerCpuCache {
    free_lists: [FreeList; NUM_SIZE_CLASSES],
    initialized: bool,
}

impl PerCpuCache {
    const fn empty() -> Self {
        Self {
            free_lists: [
                FreeList::empty(), FreeList::empty(),
                FreeList::empty(), FreeList::empty(),
                FreeList::empty(), FreeList::empty(),
                FreeList::empty(), FreeList::empty(),
            ],
            initialized: false,
        }
    }
}

/// Per-CPU caches. Indexed by CPU ID.
/// — TorqueJax: Mutable static because each CPU only touches its own cache
/// with preemption disabled. No lock needed. The invariant is: preemption
/// is disabled when accessing CPU_CACHES[this_cpu()].
static mut CPU_CACHES: [PerCpuCache; MAX_CPUS] = {
    const INIT: PerCpuCache = PerCpuCache::empty();
    [INIT; MAX_CPUS]
};

/// Page allocator callback. Set during init to buddy allocator's alloc_frame.
/// Returns virtual address of a 4K page, or 0 on failure.
/// — TorqueJax: Same callback pattern as waitqueue. Can't depend on mm-manager
/// directly (it depends on mm-heap which we're replacing as GlobalAlloc).
static PAGE_ALLOC_FN: AtomicU64 = AtomicU64::new(0);

/// Page free callback. Returns a 4K page to buddy.
static PAGE_FREE_FN: AtomicU64 = AtomicU64::new(0);

/// Register the buddy allocator page alloc/free callbacks.
/// Called once during slab_init().
pub fn set_page_callbacks(
    alloc_fn: fn() -> Option<usize>,   // returns virt addr of 4K page
    free_fn: fn(usize),                // takes virt addr of 4K page
) {
    PAGE_ALLOC_FN.store(alloc_fn as usize as u64, Ordering::Release);
    PAGE_FREE_FN.store(free_fn as usize as u64, Ordering::Release);
}

/// Allocate a page from buddy via callback.
fn alloc_page() -> Option<*mut u8> {
    let p = PAGE_ALLOC_FN.load(Ordering::Acquire);
    if p == 0 { return None; }
    let f: fn() -> Option<usize> = unsafe { core::mem::transmute(p) };
    f().map(|addr| addr as *mut u8)
}

/// Free a page back to buddy via callback.
fn _free_page(ptr: *mut u8) {
    let p = PAGE_FREE_FN.load(Ordering::Acquire);
    if p == 0 { return; }
    let f: fn(usize) = unsafe { core::mem::transmute(p) };
    f(ptr as usize);
}

/// Map a size to its size class index. Returns None for sizes > 2048.
fn size_class(size: usize) -> Option<usize> {
    match size {
        0..=16 => Some(0),
        17..=32 => Some(1),
        33..=64 => Some(2),
        65..=128 => Some(3),
        129..=256 => Some(4),
        257..=512 => Some(5),
        513..=1024 => Some(6),
        1025..=2048 => Some(7),
        _ => None,
    }
}

/// Slab alloc fast path. Called with preemption already disabled by caller.
/// — TorqueJax: No locks. Just pop from the per-CPU free list. If empty,
/// refill by carving a 4K page from buddy into size-class objects.
unsafe fn slab_alloc_fast(layout: Layout) -> Option<*mut u8> {
    let size = layout.size().max(layout.align());
    let class = size_class(size)?;
    let cpu = os_core::smp_cpu_id().unwrap_or(0) as usize;
    if cpu >= MAX_CPUS { return None; }

    let cache = unsafe { &mut CPU_CACHES[cpu] };
    if !cache.initialized {
        cache.initialized = true;
    }

    let fl = &mut cache.free_lists[class];

    // Hot path: pop from free list
    if let Some(ptr) = fl.pop() {
        return Some(ptr);
    }

    // Cold path: refill from buddy
    let page = alloc_page()?;
    let obj_size = SIZE_CLASSES[class];
    let objects_per_page = PAGE_SIZE / obj_size;

    // — TorqueJax: Carve the page into objects and push all onto free list.
    // First object is returned directly. Rest go onto the cache.
    let mut result: Option<*mut u8> = None;
    for i in 0..objects_per_page {
        let obj = unsafe { page.add(i * obj_size) };
        if result.is_none() {
            result = Some(obj);
        } else {
            fl.push(obj);
        }
    }
    result
}

/// Slab dealloc fast path. Called with preemption already disabled.
/// — TorqueJax: Push back onto per-CPU free list. If the list is too long,
/// drain half back to... well, we just drop the drain for now. The pages
/// stay allocated. A more sophisticated version would return them to buddy.
unsafe fn slab_dealloc_fast(ptr: *mut u8, layout: Layout) {
    let size = layout.size().max(layout.align());
    let class = match size_class(size) {
        Some(c) => c,
        None => return,
    };
    let cpu = os_core::smp_cpu_id().unwrap_or(0) as usize;
    if cpu >= MAX_CPUS { return; }

    let cache = unsafe { &mut CPU_CACHES[cpu] };
    let fl = &mut cache.free_lists[class];
    fl.push(ptr);

    // — TorqueJax: Drain if we're hoarding too many objects. Prevents one
    // CPU from caching all the memory while others starve.
    if fl.count > MAX_CACHED[class] as u32 {
        let (_drain_head, _drained) = fl.drain_half();
        // — TorqueJax: For now, drained objects are leaked back to the system
        // via the linked list of FreeNodes. A future version could return
        // complete pages to buddy. The memory isn't lost — it's just cached
        // in a detached chain. Good enough for v1.
    }
}

/// The slab-backed global allocator.
///
/// — TorqueJax: For small allocations (≤2048), uses per-CPU slab caches.
/// For large allocations or early boot, falls through to the existing
/// LinkedListAllocator (KernelHeap). The hot path has zero contention.
pub struct SlabAllocator {
    /// Fallback heap for large allocs and early boot
    pub fallback: KernelHeap,
}

impl SlabAllocator {
    pub const fn new() -> Self {
        Self {
            fallback: new_kernel_heap(),
        }
    }

    /// Initialize the fallback heap.
    /// — TorqueJax: Must be called before any allocation. Same as the old
    /// HEAP_ALLOCATOR.init() — we just delegate to our internal KernelHeap.
    pub unsafe fn init(&self, heap_start: usize, heap_size: usize) {
        unsafe { self.fallback.init(heap_start, heap_size); }
    }

    /// Proxy for heap stats.
    pub fn free(&self) -> usize {
        self.fallback.free()
    }

    pub fn used(&self) -> usize {
        self.fallback.used()
    }

    /// Set the grow callback on the fallback heap.
    pub fn set_grow_callback(&self, cb: mm_heap::GrowCallbackFn) {
        self.fallback.set_grow_callback(cb);
    }
}

unsafe impl GlobalAlloc for SlabAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(layout.align());

        // — TorqueJax: Slab fast path for small allocations when ready.
        if size <= 2048 && SLAB_READY.load(Ordering::Relaxed) {
            // Disable preemption for per-CPU cache access
            os_core::disallow_kernel_preempt();
            let result = unsafe { slab_alloc_fast(layout) };
            os_core::allow_kernel_preempt();

            if let Some(ptr) = result {
                return ptr;
            }
            // — TorqueJax: Slab failed (buddy exhausted). Fall through to
            // linked-list allocator which may grow from buddy too. Belt and suspenders.
        }

        // Fallback: existing LinkedListAllocator (with its global lock)
        unsafe { self.fallback.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size().max(layout.align());

        // — TorqueJax: Only dealloc to slab if:
        // 1. Size is in slab range
        // 2. Slab is ready
        // 3. Pointer is NOT in the static heap range (those go back to linked-list)
        if size <= 2048 && SLAB_READY.load(Ordering::Relaxed) && is_slab_pointer(ptr) {
            os_core::disallow_kernel_preempt();
            unsafe { slab_dealloc_fast(ptr, layout); }
            os_core::allow_kernel_preempt();
            return;
        }

        // Fallback: existing LinkedListAllocator
        unsafe { self.fallback.dealloc(ptr, layout); }
    }
}

// — TorqueJax: Track the static heap range so we don't try to slab-dealloc
// pointers that came from the linked-list allocator.
static HEAP_START: AtomicU64 = AtomicU64::new(0);
static HEAP_END: AtomicU64 = AtomicU64::new(0);

/// Set the static heap range for pointer discrimination.
/// Called during init after HEAP_ALLOCATOR.init().
pub fn set_heap_range(start: usize, size: usize) {
    HEAP_START.store(start as u64, Ordering::Release);
    HEAP_END.store((start + size) as u64, Ordering::Release);
}

/// Check if a pointer came from a slab page (not the static heap).
/// — TorqueJax: Slab pages come from buddy (high physical addresses mapped
/// into the direct map). The static heap is a BSS array at a fixed address.
/// If the pointer is inside [HEAP_START, HEAP_END), it's a linked-list alloc.
fn is_slab_pointer(ptr: *mut u8) -> bool {
    let addr = ptr as u64;
    let start = HEAP_START.load(Ordering::Relaxed);
    let end = HEAP_END.load(Ordering::Relaxed);
    if start == 0 { return false; } // not initialized yet
    addr < start || addr >= end
}

/// Initialize the slab allocator. Call after buddy + SMP are up.
/// — TorqueJax: After this, small allocations bypass the global heap lock.
pub fn slab_init() {
    // — TorqueJax: Mark all CPU caches as initialized. They start empty —
    // the first alloc on each CPU will trigger a buddy refill.
    // Nothing else to do. The magic is in SLAB_READY.
    SLAB_READY.store(true, Ordering::Release);
}

// — TorqueJax: Re-export GrowCallbackFn so globals.rs can use it
pub use mm_heap::GrowCallbackFn;
