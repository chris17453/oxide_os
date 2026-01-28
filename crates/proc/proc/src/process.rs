//! Process types and utilities
//!
//! Defines basic process-related types used throughout the system.
//! The actual process/task management is handled by the scheduler (sched crate).

use core::sync::atomic::{AtomicU32, Ordering};
use proc_traits::Pid;

/// Thread ID type (same as Pid internally but semantically different)
pub type Tid = u32;

/// Clone flags for clone() syscall
pub mod clone_flags {
    /// Share virtual memory (threads share address space)
    pub const CLONE_VM: u32 = 0x0000_0100;
    /// Share filesystem information (cwd, root)
    pub const CLONE_FS: u32 = 0x0000_0200;
    /// Share file descriptor table
    pub const CLONE_FILES: u32 = 0x0000_0400;
    /// Share signal handlers
    pub const CLONE_SIGHAND: u32 = 0x0000_0800;
    /// Create in same thread group (share PID)
    pub const CLONE_THREAD: u32 = 0x0001_0000;
    /// Set thread-local storage pointer
    pub const CLONE_SETTLS: u32 = 0x0008_0000;
    /// Store child TID at location in child memory
    pub const CLONE_CHILD_SETTID: u32 = 0x0100_0000;
    /// Clear child TID at location in child memory on exit
    pub const CLONE_CHILD_CLEARTID: u32 = 0x0020_0000;
    /// Store child TID at location in parent memory
    pub const CLONE_PARENT_SETTID: u32 = 0x0010_0000;
}

/// User credentials
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Credentials {
    /// User ID
    pub uid: u32,
    /// Group ID
    pub gid: u32,
    /// Effective user ID
    pub euid: u32,
    /// Effective group ID
    pub egid: u32,
}

impl Credentials {
    /// Root credentials
    pub const ROOT: Self = Self {
        uid: 0,
        gid: 0,
        euid: 0,
        egid: 0,
    };

    /// Create new credentials
    pub const fn new(uid: u32, gid: u32) -> Self {
        Self {
            uid,
            gid,
            euid: uid,
            egid: gid,
        }
    }

    /// Check if credentials are for root
    pub fn is_root(&self) -> bool {
        self.euid == 0
    }
}

impl Default for Credentials {
    fn default() -> Self {
        Self::ROOT
    }
}

/// Process context saved during context switch
///
/// This contains the user-mode state that needs to be saved/restored.
/// Also includes CS/SS for proper kernel-mode preemption support.
#[derive(Debug, Clone, Default)]
pub struct ProcessContext {
    /// Instruction pointer
    pub rip: u64,
    /// Stack pointer
    pub rsp: u64,
    /// Flags
    pub rflags: u64,
    /// General purpose registers
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
    /// Code segment selector (for kernel/user mode distinction)
    pub cs: u64,
    /// Stack segment selector
    pub ss: u64,
    /// FS base register (for Thread-Local Storage)
    pub fs_base: u64,
}

/// PID allocator
pub struct PidAllocator {
    next_pid: AtomicU32,
}

impl PidAllocator {
    /// Create a new PID allocator starting at PID 1
    pub const fn new() -> Self {
        Self {
            next_pid: AtomicU32::new(1),
        }
    }

    /// Allocate a new PID
    pub fn alloc(&self) -> Pid {
        self.next_pid.fetch_add(1, Ordering::Relaxed)
    }
}

/// Global PID allocator
static PID_ALLOCATOR: PidAllocator = PidAllocator::new();

/// Allocate a new PID
pub fn alloc_pid() -> Pid {
    PID_ALLOCATOR.alloc()
}
