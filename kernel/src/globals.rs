//! Global statics and shared state for the OXIDE kernel.

extern crate alloc;

use core::ops::{Deref, DerefMut};
use core::sync::atomic::AtomicBool;

use arch_traits::Arch;
use arch_x86_64::X86_64;
// — ColdCipher: KernelHeap resolves to LockedHardenedHeap when heap-hardening
// is active, plain LockedHeap otherwise. The feature flag does the heavy lifting;
// we just stop lying about which type we want.
use mm_heap::{new_kernel_heap, KernelHeap};
use mm_manager::MemoryManager;
use spin::{Mutex, MutexGuard};

/// Global kernel heap allocator — hardened by default (P3.2).
///
/// When the `heap-hardening` feature is enabled (the default), every allocation
/// gets 16-byte redzones filled with 0xFD, a trailing 0xDEAD_BEEF_CAFE_BABE
/// canary, and freed memory is poisoned with 0xDD.  Corruption is reported at
/// deallocation time via the `corruption_count` counter.
///
/// — ColdCipher: because "hope nobody overflows the heap" isn't a security policy.
#[global_allocator]
pub static HEAP_ALLOCATOR: KernelHeap = new_kernel_heap();

/// Heap size: 16 MB
pub const HEAP_SIZE: usize = 32 * 1024 * 1024; // 32MB for large executables like Python

/// Static heap storage (temporary until we have proper MM)
pub static mut HEAP_STORAGE: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

/// Global memory manager (buddy allocator - no 4GB cap)
pub static MEMORY_MANAGER: MemoryManager = MemoryManager::new();

/// Flag to track if user process has exited
pub static USER_EXITED: AtomicBool = AtomicBool::new(false);

/// Exit status from user process
pub static mut USER_EXIT_STATUS: i32 = 0;

/// Kernel PML4 physical address (for creating new address spaces)
pub static mut KERNEL_PML4: u64 = 0;

/// — CrashBloom: Golden reference for PML4[256] — the direct physical map entry.
/// Captured at boot, verified on every context switch. If a process's PML4[256]
/// doesn't match this, its kernel entries are corrupted and switching to it would
/// triple-fault (exception handlers can't even load from unmapped kernel addresses).
/// This is the canary that screams before the mine explodes.
pub static mut KERNEL_PML4_256_ENTRY: u64 = 0;

/// Mutex that disables preemption while locked so the scheduler interrupt
/// doesn't deadlock trying to take a lock already held by the preempted task.
pub struct InterruptMutex<T> {
    inner: Mutex<T>,
}

impl<T> InterruptMutex<T> {
    /// Create a new interrupt-safe mutex
    pub const fn new(value: T) -> Self {
        Self {
            inner: Mutex::new(value),
        }
    }

    /// Lock the mutex, disabling interrupts if they were previously enabled
    pub fn lock(&self) -> InterruptMutexGuard<'_, T> {
        let interrupts_were_enabled = X86_64::interrupts_enabled();
        if interrupts_were_enabled {
            X86_64::disable_interrupts();
        }
        InterruptMutexGuard {
            guard: Some(self.inner.lock()),
            interrupts_were_enabled,
        }
    }
}

/// RAII guard for InterruptMutex that re-enables interrupts on drop if needed
pub struct InterruptMutexGuard<'a, T> {
    guard: Option<MutexGuard<'a, T>>,
    interrupts_were_enabled: bool,
}

impl<T> Deref for InterruptMutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.guard.as_ref().unwrap()
    }
}

impl<T> DerefMut for InterruptMutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.guard.as_mut().unwrap()
    }
}

impl<T> Drop for InterruptMutexGuard<'_, T> {
    fn drop(&mut self) {
        // First drop the inner guard to release the lock
        self.guard.take();
        // Then re-enable interrupts if they were enabled before
        if self.interrupts_were_enabled {
            X86_64::enable_interrupts();
        }
    }
}

/// Full parent context for returning from child process
/// Stores all registers so parent can resume with correct state
#[derive(Clone)]
#[repr(C)]
pub struct ParentContext {
    pub pid: u64, // Changed from u32 to u64 for consistent 8-byte alignment
    pub pml4: u64,
    pub rip: u64,
    pub rsp: u64,
    pub rflags: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
}

/// Saved parent context for returning from child process
pub static PARENT_CONTEXT: Mutex<Option<ParentContext>> = Mutex::new(None);

/// Flag indicating a child has exited and we should return to parent
pub static CHILD_DONE: AtomicBool = AtomicBool::new(false);
