//! Global statics and shared state for the OXIDE kernel.

extern crate alloc;

use alloc::vec::Vec;
use core::sync::atomic::AtomicBool;

use mm_frame::BitmapFrameAllocator;
use mm_heap::LockedHeap;
use proc_traits::Pid;
use spin::Mutex;

/// Global kernel heap allocator
#[global_allocator]
pub static HEAP_ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Heap size: 16 MB
pub const HEAP_SIZE: usize = 16 * 1024 * 1024;

/// Static heap storage (temporary until we have proper MM)
pub static mut HEAP_STORAGE: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

/// Global frame allocator
pub static FRAME_ALLOCATOR: BitmapFrameAllocator = BitmapFrameAllocator::new();

/// Flag to track if user process has exited
pub static USER_EXITED: AtomicBool = AtomicBool::new(false);

/// Exit status from user process
pub static mut USER_EXIT_STATUS: i32 = 0;

/// Kernel PML4 physical address (for creating new address spaces)
pub static mut KERNEL_PML4: u64 = 0;

/// Ready queue - processes that are ready to run
/// The scheduler picks from this queue on each timer tick
pub static READY_QUEUE: Mutex<Vec<Pid>> = Mutex::new(Vec::new());

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
