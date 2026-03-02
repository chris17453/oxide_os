//! Memory allocator — mmap-backed bump allocator for std's GlobalAlloc.
//!
//! — ByteRiot: Two-tier design: 256KB BSS bootstrap for early allocs
//! before mmap is available, then 2MB mmap arenas for the real work.
//! We never free. Life's too short for a real allocator in a runtime crate.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

const BOOTSTRAP_SIZE: usize = 256 * 1024; // 256KB
const ARENA_SIZE: usize = 2 * 1024 * 1024; // 2MB
const MAX_ARENAS: usize = 32;

#[repr(C, align(16))]
struct BootstrapHeap {
    data: UnsafeCell<[u8; BOOTSTRAP_SIZE]>,
}

unsafe impl Sync for BootstrapHeap {}

static BOOTSTRAP: BootstrapHeap = BootstrapHeap {
    data: UnsafeCell::new([0; BOOTSTRAP_SIZE]),
};
static BOOTSTRAP_POS: AtomicUsize = AtomicUsize::new(0);

static ARENA_BASE: AtomicUsize = AtomicUsize::new(0);
static ARENA_POS: AtomicUsize = AtomicUsize::new(0);
static ARENA_COUNT: AtomicUsize = AtomicUsize::new(0);

/// The bump allocator used by std's global allocator
pub struct OxideAllocator;

unsafe impl GlobalAlloc for OxideAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // — ByteRiot: Try bootstrap heap first (lock-free CAS loop)
        let pos = BOOTSTRAP_POS.load(Ordering::Relaxed);
        let aligned = (pos + align - 1) & !(align - 1);
        let new_pos = aligned + size;

        if new_pos <= BOOTSTRAP_SIZE {
            if BOOTSTRAP_POS
                .compare_exchange(pos, new_pos, Ordering::SeqCst, Ordering::Relaxed)
                .is_ok()
            {
                return unsafe { (BOOTSTRAP.data.get() as *mut u8).add(aligned) };
            }
        }

        // — ByteRiot: Bootstrap full or CAS failed, go to mmap arenas
        unsafe { arena_alloc(size, align) }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // — ByteRiot: Bump allocator. We don't free. Deal with it.
    }
}

unsafe fn arena_alloc(size: usize, align: usize) -> *mut u8 {
    loop {
        let base = ARENA_BASE.load(Ordering::Acquire);
        if base == 0 {
            if !new_arena() {
                return core::ptr::null_mut();
            }
            continue;
        }

        let pos = ARENA_POS.load(Ordering::Relaxed);
        let aligned = (pos + align - 1) & !(align - 1);
        let new_pos = aligned + size;

        if new_pos > ARENA_SIZE {
            if !new_arena() {
                return core::ptr::null_mut();
            }
            continue;
        }

        if ARENA_POS
            .compare_exchange_weak(pos, new_pos, Ordering::SeqCst, Ordering::Relaxed)
            .is_ok()
        {
            return (base as *mut u8).add(aligned);
        }
    }
}

fn new_arena() -> bool {
    let count = ARENA_COUNT.load(Ordering::Relaxed);
    if count >= MAX_ARENAS {
        return false;
    }

    let ptr = unsafe {
        crate::syscall::syscall6(
            crate::nr::MMAP,
            0,           // addr = NULL (let kernel pick)
            ARENA_SIZE,  // length
            0x3,         // PROT_READ | PROT_WRITE
            0x22,        // MAP_PRIVATE | MAP_ANONYMOUS
            usize::MAX,  // fd = -1
            0,           // offset
        )
    };

    if ptr < 0 || ptr as usize == usize::MAX {
        return false;
    }

    ARENA_POS.store(0, Ordering::Release);
    ARENA_BASE.store(ptr as usize, Ordering::Release);
    ARENA_COUNT.fetch_add(1, Ordering::SeqCst);
    true
}

/// Raw mmap wrapper for external use (e.g., std's System allocator)
pub unsafe fn mmap(addr: usize, len: usize, prot: i32, flags: i32, fd: i32, offset: usize) -> *mut u8 {
    let ret = unsafe {
        crate::syscall::syscall6(
            crate::nr::MMAP,
            addr,
            len,
            prot as usize,
            flags as usize,
            fd as usize,
            offset,
        )
    };
    if ret < 0 {
        core::ptr::null_mut()
    } else {
        ret as *mut u8
    }
}

/// Raw munmap wrapper
pub unsafe fn munmap(addr: *mut u8, len: usize) -> i32 {
    unsafe {
        crate::syscall::syscall2(crate::nr::MUNMAP, addr as usize, len) as i32
    }
}
