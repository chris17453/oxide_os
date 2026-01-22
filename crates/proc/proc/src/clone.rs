//! Clone implementation for thread creation
//!
//! Implements the clone() system call which can create either:
//! - A new process (like fork, with separate address space)
//! - A new thread (shares address space with parent)

use crate::fork::{ForkError, do_fork};
use crate::{
    Process, ProcessContext, Tid, UserAddressSpace, alloc_pid, clone_flags::*, process_table,
};
use alloc::string::ToString;
use alloc::sync::Arc;
use mm_traits::FrameAllocator;
use os_core::VirtAddr;
use proc_traits::Pid;
use spin::Mutex;

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

/// Clone the current process/thread
///
/// Creates a new process or thread based on the flags:
/// - No CLONE_VM: Creates a new process (like fork)
/// - CLONE_VM | CLONE_THREAD: Creates a new thread
///
/// # Arguments
/// * `parent_pid` - PID of the calling process
/// * `parent_context` - Saved context of parent
/// * `args` - Clone arguments including flags
/// * `allocator` - Frame allocator for kernel stack
///
/// # Returns
/// Child TID to parent, 0 to child, or error
pub fn do_clone<A: FrameAllocator>(
    parent_pid: Pid,
    parent_context: &ProcessContext,
    args: &CloneArgs,
    allocator: &A,
) -> Result<Tid, CloneError> {
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

    // If not sharing VM, this is like fork
    if flags & CLONE_VM == 0 {
        let child_pid = do_fork(parent_pid, parent_context, allocator)?;
        return Ok(child_pid);
    }

    // Creating a thread (shares address space)
    let table = process_table();

    // Get parent process
    let parent_arc = table.get(parent_pid).ok_or(CloneError::ParentNotFound)?;
    let mut parent = parent_arc.lock();

    // Allocate new TID (using PID allocator since TIDs are unique across system)
    let child_tid = alloc_pid();

    // Get parent's TGID (all threads in group share this)
    let tgid = parent.tgid();

    // Allocate kernel stack for the new thread
    let kernel_stack_size = parent.kernel_stack_size();
    let kernel_stack_pages = kernel_stack_size / 4096;
    let kernel_stack_phys = allocator
        .alloc_frames(kernel_stack_pages)
        .ok_or(CloneError::OutOfMemory)?;

    // Create or get shared address space
    let shared_address_space = if let Some(shared) = parent.shared_address_space() {
        Arc::clone(shared)
    } else {
        // Parent is the thread group leader, create shared wrapper
        // For simplicity, we create a reference to parent's address space
        // In a full implementation, we'd need to properly share the address space
        let parent_as = unsafe {
            UserAddressSpace::from_raw(parent.address_space().pml4_phys(), alloc::vec![])
        };
        Arc::new(Mutex::new(parent_as))
    };

    // Optionally share file descriptor table
    let shared_fd_table = if flags & CLONE_FILES != 0 {
        if let Some(shared) = parent.shared_fd_table() {
            Some(Arc::clone(shared))
        } else {
            // Create shared FD table from parent's
            Some(Arc::new(Mutex::new(parent.clone_fd_table())))
        }
    } else {
        None
    };

    // Get child stack - use provided stack or inherit
    let child_stack = if args.stack != 0 {
        VirtAddr::new(args.stack)
    } else {
        parent.user_stack_top()
    };

    // Copy signal handlers if CLONE_SIGHAND
    let sigactions = if flags & CLONE_SIGHAND != 0 {
        *parent.sigactions()
    } else {
        *parent.sigactions() // Clone handlers anyway (they're just defaults)
    };

    // Create the new thread
    let mut thread = Process::new_thread(
        child_tid,
        tgid,
        parent.ppid(),
        kernel_stack_phys,
        kernel_stack_size,
        VirtAddr::new(parent_context.rip), // Entry point is return address
        child_stack,
        shared_address_space,
        shared_fd_table,
        *parent.credentials(),
        parent.pgid(),
        parent.sid(),
        sigactions,
        parent.cwd().to_string(),
    );

    // Copy parent's context to child (will return 0 to child)
    let mut child_context = parent_context.clone();
    child_context.rax = 0; // clone returns 0 to child
    if args.stack != 0 {
        child_context.rsp = args.stack; // Use new stack
    }
    *thread.context_mut() = child_context;

    // Set TLS if requested
    if flags & CLONE_SETTLS != 0 {
        thread.set_tls(args.tls);
    }

    // Set clear_child_tid if requested
    if flags & CLONE_CHILD_CLEARTID != 0 {
        thread.set_clear_child_tid(args.child_tid);
    }

    // Track kernel stack frame for cleanup
    thread.add_owned_frame(kernel_stack_phys);

    // Add thread to parent's thread group
    if flags & CLONE_THREAD != 0 {
        parent.add_thread(child_tid);
    }

    // Store child TID in parent's memory if requested
    if flags & CLONE_PARENT_SETTID != 0 && args.parent_tid != 0 {
        // Write TID to parent's address space
        // Safety: We validated this is user space in syscall handler
        if args.parent_tid < 0x0000_8000_0000_0000 {
            unsafe {
                let ptr = args.parent_tid as *mut u32;
                *ptr = child_tid;
            }
        }
    }

    // Store child TID in child's memory if requested
    if flags & CLONE_CHILD_SETTID != 0 && args.child_tid != 0 {
        // This will be written when the child starts running
        // For now, write it directly since we share address space
        if args.child_tid < 0x0000_8000_0000_0000 {
            unsafe {
                let ptr = args.child_tid as *mut u32;
                *ptr = child_tid;
            }
        }
    }

    // Release parent lock before adding child to table
    drop(parent);

    // Add thread to process table
    table.add(thread);

    Ok(child_tid)
}
