//! Process structure and management
//!
//! Defines the Process type and process-related operations.

// Debug output via serial port
fn debug_print(msg: &str) {
    const SERIAL_PORT: u16 = 0x3F8;
    for byte in msg.bytes() {
        unsafe {
            // Wait for transmit buffer to be empty
            let mut status: u8;
            loop {
                core::arch::asm!(
                    "in al, dx",
                    out("al") status,
                    in("dx") SERIAL_PORT + 5,
                    options(nomem, nostack)
                );
                if status & 0x20 != 0 {
                    break;
                }
                core::hint::spin_loop();
            }
            // Send byte
            core::arch::asm!(
                "out dx, al",
                in("al") byte,
                in("dx") SERIAL_PORT,
                options(nomem, nostack)
            );
        }
    }
}

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use efflux_core::{PhysAddr, VirtAddr};
use efflux_proc_traits::{Pid, ProcessState};
use efflux_signal::{PendingSignals, SigAction, SigSet, NSIG};
use efflux_vfs::FdTable;
use spin::{Mutex, RwLock};

use crate::UserAddressSpace;

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
#[derive(Debug, Clone, Default)]
pub struct ProcessContext {
    /// User instruction pointer
    pub rip: u64,
    /// User stack pointer
    pub rsp: u64,
    /// User flags
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
}

/// Process structure
///
/// Represents a user process with its own address space.
pub struct Process {
    /// Process ID
    pid: Pid,
    /// Parent process ID (0 for init)
    ppid: Pid,
    /// Process state
    state: ProcessState,
    /// Exit status (valid when state is Zombie)
    exit_status: i32,
    /// User credentials
    credentials: Credentials,
    /// Process group ID
    pgid: Pid,
    /// Session ID
    sid: Pid,
    /// Address space
    address_space: UserAddressSpace,
    /// Saved context for user mode
    context: ProcessContext,
    /// Kernel stack for this process (physical address of top)
    kernel_stack: PhysAddr,
    /// Kernel stack size
    kernel_stack_size: usize,
    /// User stack top
    user_stack_top: VirtAddr,
    /// Entry point
    entry_point: VirtAddr,
    /// Child PIDs
    children: Vec<Pid>,
    /// Physical frames owned by this process (for COW tracking)
    owned_frames: Vec<PhysAddr>,
    /// File descriptor table
    fd_table: FdTable,
    /// Signal mask (blocked signals)
    signal_mask: SigSet,
    /// Pending signals
    pending_signals: PendingSignals,
    /// Signal actions (handlers)
    sigactions: [SigAction; NSIG],
    /// Current working directory
    cwd: String,
}

/// Configuration for creating a new process
/// Used to reduce argument count and avoid stack argument corruption
#[derive(Clone, Copy)]
pub struct ProcessConfig {
    pub kernel_stack: PhysAddr,
    pub kernel_stack_size: usize,
    pub entry_point: VirtAddr,
    pub user_stack_top: VirtAddr,
}

impl Process {
    /// Create a new process
    ///
    /// Uses ProcessConfig to avoid stack arguments (ABI limitation)
    pub fn new(
        pid: Pid,
        ppid: Pid,
        address_space: UserAddressSpace,
        config: &ProcessConfig,
    ) -> Self {
        Self {
            pid,
            ppid,
            state: ProcessState::Ready,
            exit_status: 0,
            credentials: Credentials::default(),
            pgid: pid,  // New process is its own process group leader
            sid: pid,   // New process starts a new session
            address_space,
            context: ProcessContext::default(),
            kernel_stack: config.kernel_stack,
            kernel_stack_size: config.kernel_stack_size,
            user_stack_top: config.user_stack_top,
            entry_point: config.entry_point,
            children: Vec::new(),
            owned_frames: Vec::new(),
            fd_table: FdTable::new(),
            signal_mask: SigSet::empty(),
            pending_signals: PendingSignals::new(),
            sigactions: [SigAction::new(); NSIG],
            cwd: String::from("/"),
        }
    }

    /// Get the process ID
    pub fn pid(&self) -> Pid {
        self.pid
    }

    /// Get the parent process ID
    pub fn ppid(&self) -> Pid {
        self.ppid
    }

    /// Get the process state
    pub fn state(&self) -> ProcessState {
        self.state
    }

    /// Set the process state
    pub fn set_state(&mut self, state: ProcessState) {
        self.state = state;
    }

    /// Get the exit status (only valid when state is Zombie)
    pub fn exit_status(&self) -> i32 {
        self.exit_status
    }

    /// Set the exit status and transition to Zombie state
    pub fn exit(&mut self, status: i32) {
        self.exit_status = status;
        self.state = ProcessState::Zombie;
    }

    /// Get credentials
    pub fn credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// Set credentials
    pub fn set_credentials(&mut self, creds: Credentials) {
        self.credentials = creds;
    }

    /// Get process group ID
    pub fn pgid(&self) -> Pid {
        self.pgid
    }

    /// Set process group ID
    pub fn set_pgid(&mut self, pgid: Pid) {
        self.pgid = pgid;
    }

    /// Get session ID
    pub fn sid(&self) -> Pid {
        self.sid
    }

    /// Set session ID
    pub fn set_sid(&mut self, sid: Pid) {
        self.sid = sid;
    }

    /// Get a reference to the address space
    pub fn address_space(&self) -> &UserAddressSpace {
        &self.address_space
    }

    /// Get a mutable reference to the address space
    pub fn address_space_mut(&mut self) -> &mut UserAddressSpace {
        &mut self.address_space
    }

    /// Get the saved context
    pub fn context(&self) -> &ProcessContext {
        &self.context
    }

    /// Get a mutable reference to the saved context
    pub fn context_mut(&mut self) -> &mut ProcessContext {
        &mut self.context
    }

    /// Get the kernel stack physical address
    pub fn kernel_stack(&self) -> PhysAddr {
        self.kernel_stack
    }

    /// Get the kernel stack size
    pub fn kernel_stack_size(&self) -> usize {
        self.kernel_stack_size
    }

    /// Get the user stack top
    pub fn user_stack_top(&self) -> VirtAddr {
        self.user_stack_top
    }

    /// Get the entry point
    pub fn entry_point(&self) -> VirtAddr {
        self.entry_point
    }

    /// Set the entry point
    pub fn set_entry_point(&mut self, entry: VirtAddr) {
        self.entry_point = entry;
    }

    /// Set the user stack top
    pub fn set_user_stack_top(&mut self, stack_top: VirtAddr) {
        self.user_stack_top = stack_top;
    }

    /// Add a child PID
    pub fn add_child(&mut self, child_pid: Pid) {
        self.children.push(child_pid);
    }

    /// Remove a child PID
    pub fn remove_child(&mut self, child_pid: Pid) {
        self.children.retain(|&p| p != child_pid);
    }

    /// Get children PIDs
    pub fn children(&self) -> &[Pid] {
        &self.children
    }

    /// Add an owned frame (for cleanup on process exit)
    pub fn add_owned_frame(&mut self, frame: PhysAddr) {
        self.owned_frames.push(frame);
    }

    /// Get owned frames
    pub fn owned_frames(&self) -> &[PhysAddr] {
        &self.owned_frames
    }

    /// Take ownership of all owned frames (for cleanup)
    pub fn take_owned_frames(&mut self) -> Vec<PhysAddr> {
        core::mem::take(&mut self.owned_frames)
    }

    /// Get a reference to the file descriptor table
    pub fn fd_table(&self) -> &FdTable {
        &self.fd_table
    }

    /// Get a mutable reference to the file descriptor table
    pub fn fd_table_mut(&mut self) -> &mut FdTable {
        &mut self.fd_table
    }

    /// Clone fd table for fork
    pub fn clone_fd_table(&self) -> FdTable {
        self.fd_table.clone_for_fork()
    }

    /// Set fd table (for fork)
    pub fn set_fd_table(&mut self, fd_table: FdTable) {
        self.fd_table = fd_table;
    }

    /// Get the signal mask (blocked signals)
    pub fn signal_mask(&self) -> &SigSet {
        &self.signal_mask
    }

    /// Get a mutable reference to the signal mask
    pub fn signal_mask_mut(&mut self) -> &mut SigSet {
        &mut self.signal_mask
    }

    /// Set the signal mask
    pub fn set_signal_mask(&mut self, mask: SigSet) {
        self.signal_mask = mask;
    }

    /// Get pending signals
    pub fn pending_signals(&self) -> &PendingSignals {
        &self.pending_signals
    }

    /// Get a mutable reference to pending signals
    pub fn pending_signals_mut(&mut self) -> &mut PendingSignals {
        &mut self.pending_signals
    }

    /// Get a signal action
    pub fn sigaction(&self, sig: i32) -> Option<&SigAction> {
        if sig >= 1 && sig <= NSIG as i32 {
            Some(&self.sigactions[(sig - 1) as usize])
        } else {
            None
        }
    }

    /// Set a signal action
    pub fn set_sigaction(&mut self, sig: i32, action: SigAction) {
        if sig >= 1 && sig <= NSIG as i32 {
            self.sigactions[(sig - 1) as usize] = action;
        }
    }

    /// Get the sigactions array (for fork)
    pub fn sigactions(&self) -> &[SigAction; NSIG] {
        &self.sigactions
    }

    /// Set sigactions (for fork/exec)
    pub fn set_sigactions(&mut self, actions: [SigAction; NSIG]) {
        self.sigactions = actions;
    }

    /// Check if there are any deliverable signals
    pub fn has_pending_signals(&self) -> bool {
        self.pending_signals.has_deliverable(&self.signal_mask)
    }

    /// Add a pending signal
    pub fn send_signal(&mut self, sig: i32, info: Option<efflux_signal::SigInfo>) {
        self.pending_signals.add(sig, info);
    }

    /// Get the current working directory
    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    /// Set the current working directory
    pub fn set_cwd(&mut self, path: String) {
        self.cwd = path;
    }

    /// Clone cwd (for fork)
    pub fn clone_cwd(&self) -> String {
        self.cwd.clone()
    }
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

/// Process table for managing all processes
pub struct ProcessTable {
    /// Map of PID to process
    processes: RwLock<BTreeMap<Pid, Arc<Mutex<Process>>>>,
    /// Currently running process PID (per-CPU in SMP, single for now)
    current_pid: AtomicU32,
}

impl ProcessTable {
    /// Create a new process table
    pub const fn new() -> Self {
        Self {
            processes: RwLock::new(BTreeMap::new()),
            current_pid: AtomicU32::new(0),
        }
    }

    /// Add a process to the table
    pub fn add(&self, process: Process) -> Arc<Mutex<Process>> {
        let pid = process.pid();
        let mutex = Mutex::new(process);
        // Disable interrupts during Arc allocation to prevent potential heap deadlock
        unsafe { core::arch::asm!("cli", options(nomem, nostack)) };
        let arc = Arc::new(mutex);
        unsafe { core::arch::asm!("sti", options(nomem, nostack)) };
        self.processes.write().insert(pid, Arc::clone(&arc));
        arc
    }

    /// Get a process by PID
    pub fn get(&self, pid: Pid) -> Option<Arc<Mutex<Process>>> {
        debug_print("[PTABLE] get() - acquiring read lock...\n");
        let guard = self.processes.read();
        debug_print("[PTABLE] get() - got read lock\n");
        let result = guard.get(&pid).cloned();
        debug_print("[PTABLE] get() - releasing read lock\n");
        result
    }

    /// Remove a process from the table
    pub fn remove(&self, pid: Pid) -> Option<Arc<Mutex<Process>>> {
        self.processes.write().remove(&pid)
    }

    /// Get the current process PID
    pub fn current_pid(&self) -> Pid {
        self.current_pid.load(Ordering::Relaxed)
    }

    /// Set the current process PID
    pub fn set_current_pid(&self, pid: Pid) {
        self.current_pid.store(pid, Ordering::Relaxed);
    }

    /// Get the current process
    pub fn current(&self) -> Option<Arc<Mutex<Process>>> {
        let pid = self.current_pid();
        if pid == 0 {
            None
        } else {
            self.get(pid)
        }
    }

    /// Get all process PIDs
    pub fn all_pids(&self) -> Vec<Pid> {
        self.processes.read().keys().copied().collect()
    }

    /// Get number of processes
    pub fn count(&self) -> usize {
        self.processes.read().len()
    }

    /// Find zombie children of a process
    pub fn find_zombie_children(&self, ppid: Pid) -> Vec<Pid> {
        self.processes
            .read()
            .iter()
            .filter_map(|(&pid, proc)| {
                let p = proc.lock();
                if p.ppid() == ppid && p.state() == ProcessState::Zombie {
                    Some(pid)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Find any children of a process
    pub fn find_children(&self, ppid: Pid) -> Vec<Pid> {
        self.processes
            .read()
            .iter()
            .filter_map(|(&pid, proc)| {
                if proc.lock().ppid() == ppid {
                    Some(pid)
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Global process table
static PROCESS_TABLE: ProcessTable = ProcessTable::new();

/// Get the global process table
pub fn process_table() -> &'static ProcessTable {
    &PROCESS_TABLE
}
