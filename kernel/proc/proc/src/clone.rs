//! Clone implementation for thread creation
//!
//! Implements the clone() system call which can create either:
//! - A new process (like fork, with separate address space)
//! - A new thread (shares address space with parent)
//!
//! This module returns CloneResult which contains all data needed
//! to create a Task. The actual Task creation is done by the kernel
//! which has access to the scheduler.

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use mm_traits::FrameAllocator;
use os_core::{PhysAddr, VirtAddr};
use proc_traits::Pid;
use signal::{NSIG, SigAction};
use spin::Mutex;

use crate::fork::{ForkError, ForkResult, do_fork};
use crate::{ProcessContext, ProcessMeta, Tid, UserAddressSpace, alloc_pid, clone_flags::*};

/// Error during clone
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloneError {
    /// Out of memory
    OutOfMemory,
    /// Parent process not found
    ParentNotFound,
    /// Invalid flags combination
    InvalidFlags,
    /// Internal error
    Internal,
    /// Fork error
    ForkError(ForkError),
}

impl From<ForkError> for CloneError {
    fn from(e: ForkError) -> Self {
        CloneError::ForkError(e)
    }
}

/// Arguments for clone syscall
#[derive(Debug, Clone)]
pub struct CloneArgs {
    /// Clone flags
    pub flags: u32,
    /// Child stack pointer (for threads)
    pub stack: u64,
    /// Parent TID store location
    pub parent_tid: u64,
    /// Child TID store location
    pub child_tid: u64,
    /// TLS (thread-local storage) pointer
    pub tls: u64,
}

impl Default for CloneArgs {
    fn default() -> Self {
        Self {
            flags: 0,
            stack: 0,
            parent_tid: 0,
            child_tid: 0,
            tls: 0,
        }
    }
}

/// Result of a clone operation that creates a new thread
pub struct CloneResult {
    /// Thread ID of the new thread
    pub child_tid: Tid,
    /// Thread Group ID (shared with parent)
    pub tgid: Pid,
    /// Parent PID
    pub ppid: Pid,
    /// Child's initial context
    pub child_context: ProcessContext,
    /// Physical address of kernel stack (above the guard frame)
    pub kernel_stack_phys: PhysAddr,
    /// Size of kernel stack (NOT including guard frame)
    pub kernel_stack_size: usize,
    /// Physical address of the guard frame (alloc_base, one page below kernel_stack_phys).
    /// The kernel crate calls kstack_guard::unmap_guard_page(guard_phys) after CloneResult
    /// is returned to clear the PTE. Also added to child_meta.guard_pages for Drop cleanup.
    /// — BlackLatch: Zero means no guard (e.g. early-boot init stack before guard support).
    pub guard_phys: PhysAddr,
    /// Shared address space (Arc for thread sharing)
    pub shared_address_space: Arc<Mutex<UserAddressSpace>>,
    /// Shared fd table (if CLONE_FILES)
    pub shared_fd_table: Option<Arc<Mutex<vfs::FdTable>>>,
    /// Credentials (copied from parent)
    pub credentials: crate::Credentials,
    /// Process group ID
    pub pgid: Pid,
    /// Session ID
    pub sid: Pid,
    /// Signal actions (handlers)
    pub sigactions: [SigAction; NSIG],
    /// Current working directory
    pub cwd: String,
    /// TLS value (if CLONE_SETTLS)
    pub tls: u64,
    /// Clear child TID address (if CLONE_CHILD_CLEARTID)
    pub clear_child_tid: u64,
    /// Parent TID address to write (if CLONE_PARENT_SETTID)
    pub parent_tid_addr: u64,
    /// Child TID address to write (if CLONE_CHILD_SETTID)
    pub child_tid_addr: u64,
}

/// Clone the current process/thread
///
/// Creates a new process or thread based on the flags:
/// - No CLONE_VM: Creates a new process (like fork) - returns Err with ForkResult embedded
/// - CLONE_VM | CLONE_THREAD: Creates a new thread - returns Ok(CloneResult)
///
/// # Arguments
/// * `parent_pid` - PID of the calling process
/// * `parent_meta` - Parent's ProcessMeta
/// * `parent_context` - Saved context of parent
/// * `args` - Clone arguments including flags
/// * `allocator` - Frame allocator for kernel stack
/// * `kernel_stack_size` - Size of kernel stack to allocate
///
/// # Returns
/// Ok(CloneResult) for thread creation, or Err(CloneError::ForkError) containing
/// a ForkResult if this is a fork operation (no CLONE_VM).
pub fn do_clone<A: FrameAllocator>(
    parent_pid: Pid,
    parent_meta: &ProcessMeta,
    parent_context: &ProcessContext,
    args: &CloneArgs,
    allocator: &A,
    kernel_stack_size: usize,
) -> Result<CloneResult, CloneError> {
    let flags = args.flags;

    // Validate flag combinations
    // CLONE_THREAD requires CLONE_VM
    if (flags & CLONE_THREAD != 0) && (flags & CLONE_VM == 0) {
        return Err(CloneError::InvalidFlags);
    }

    // CLONE_SIGHAND requires CLONE_VM
    if (flags & CLONE_SIGHAND != 0) && (flags & CLONE_VM == 0) {
        return Err(CloneError::InvalidFlags);
    }

    // If not sharing VM, this is like fork - caller should use do_fork instead
    if flags & CLONE_VM == 0 {
        return Err(CloneError::InvalidFlags);
    }

    // Creating a thread (shares address space)

    // Allocate new TID (using PID allocator since TIDs are unique across system)
    let child_tid = alloc_pid();

    // Get parent's TGID (all threads in group share this)
    let tgid = parent_meta.tgid;

    // — BlackLatch: Allocate (stack_pages + 1) contiguous frames for this thread.
    // Frame 0 = guard page (PTE cleared by caller in process.rs).
    // Frames 1..N = actual kernel stack.
    // The guard frame physically belongs to this allocation; cleanup via
    // owned_frames uses total_pages so the buddy block is freed correctly.
    let kernel_stack_pages = kernel_stack_size / 4096;
    let total_pages = kernel_stack_pages + 1;
    let alloc_base = allocator
        .alloc_frames(total_pages)
        .ok_or(CloneError::OutOfMemory)?;

    // Guard = alloc_base; real stack starts one page above.
    let guard_phys = alloc_base;
    let kernel_stack_phys = PhysAddr::new(alloc_base.as_u64() + 4096);

    // Create or get shared address space
    let shared_address_space = if let Some(shared) = &parent_meta.shared_address_space {
        Arc::clone(shared)
    } else {
        // Parent is the thread group leader, create shared wrapper
        let parent_as = unsafe {
            UserAddressSpace::from_raw(parent_meta.address_space.pml4_phys(), alloc::vec![], mm_vma::VmAreaList::new())
        };
        Arc::new(Mutex::new(parent_as))
    };

    // Optionally share file descriptor table
    let shared_fd_table = if flags & CLONE_FILES != 0 {
        if let Some(shared) = &parent_meta.shared_fd_table {
            Some(Arc::clone(shared))
        } else {
            // Create shared FD table from parent's
            Some(Arc::new(Mutex::new(parent_meta.fd_table.clone_for_fork())))
        }
    } else {
        None
    };

    // Get child stack - use provided stack or inherit
    let child_stack = if args.stack != 0 {
        args.stack
    } else {
        parent_context.rsp
    };

    // Copy signal handlers
    let sigactions = parent_meta.sigactions;

    // Copy parent's context to child (will return 0 to child)
    let mut child_context = parent_context.clone();
    child_context.rax = 0; // clone returns 0 to child
    if args.stack != 0 {
        child_context.rsp = args.stack; // Use new stack
    }

    // Set TLS if requested
    let tls = if flags & CLONE_SETTLS != 0 {
        args.tls
    } else {
        0
    };

    // Set clear_child_tid if requested
    let clear_child_tid = if flags & CLONE_CHILD_CLEARTID != 0 {
        args.child_tid
    } else {
        0
    };

    // Determine addresses to write TID to
    let parent_tid_addr = if flags & CLONE_PARENT_SETTID != 0 {
        args.parent_tid
    } else {
        0
    };

    let child_tid_addr = if flags & CLONE_CHILD_SETTID != 0 {
        args.child_tid
    } else {
        0
    };

    Ok(CloneResult {
        child_tid,
        tgid,
        ppid: parent_pid,
        child_context,
        kernel_stack_phys,
        kernel_stack_size,
        guard_phys,
        shared_address_space,
        shared_fd_table,
        credentials: parent_meta.credentials,
        pgid: parent_meta.pgid,
        sid: parent_meta.sid,
        sigactions,
        cwd: parent_meta.cwd.clone(),
        tls,
        clear_child_tid,
        parent_tid_addr,
        child_tid_addr,
    })
}
