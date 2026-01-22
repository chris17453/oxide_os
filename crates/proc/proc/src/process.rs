//! Process structure and management
//!
//! Defines the Process type and process-related operations.
//!
//! In OXIDE, threads are implemented using the Linux model where threads
//! are processes that share certain resources. Each thread has a unique TID
//! (Thread ID) but shares a TGID (Thread Group ID) with other threads in
//! the same thread group. The TGID is what userspace sees as the "PID".

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};
use os_core::{PhysAddr, VirtAddr};
use proc_traits::{Pid, ProcessState};
use signal::{NSIG, PendingSignals, SigAction, SigSet};
use spin::{Mutex, RwLock};
use vfs::FdTable;

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
/// Represents a user process/thread with its own context.
/// Threads are processes that share address space and other resources.
pub struct Process {
    /// Process/Thread ID - unique identifier for this task
    pid: Pid,
    /// Thread ID - same as pid for the main thread, unique per thread
    tid: Tid,
    /// Thread Group ID - shared by all threads in a process (the "PID" seen by userspace)
    tgid: Pid,
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
    /// Address space (shared between threads via Arc)
    address_space: UserAddressSpace,
    /// Shared address space reference (for threads)
    shared_address_space: Option<Arc<Mutex<UserAddressSpace>>>,
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
    /// File descriptor table (may be shared between threads)
    fd_table: FdTable,
    /// Shared file descriptor table (for threads with CLONE_FILES)
    shared_fd_table: Option<Arc<Mutex<FdTable>>>,
    /// Signal mask (blocked signals)
    signal_mask: SigSet,
    /// Pending signals
    pending_signals: PendingSignals,
    /// Signal actions (handlers)
    sigactions: [SigAction; NSIG],
    /// Current working directory
    cwd: String,
    /// Command line arguments (for /proc/[pid]/cmdline)
    cmdline: Vec<String>,
    /// Environment variables (for /proc/[pid]/environ)
    environ: Vec<String>,
    /// Thread-local storage pointer (for CLONE_SETTLS)
    tls: u64,
    /// Address to clear and futex wake on thread exit (for CLONE_CHILD_CLEARTID)
    clear_child_tid: u64,
    /// Is this the thread group leader?
    is_thread_leader: bool,
    /// Thread group members (TIDs of threads in this group, only valid for leader)
    thread_group: Vec<Tid>,
    /// Process nice value (priority: -20 to +19, default 0)
    nice: i32,
    /// Alarm remaining time in seconds (0 = no alarm)
    alarm_remaining: u32,
    /// Interval timer - interval seconds
    itimer_interval_sec: i64,
    /// Interval timer - interval microseconds
    itimer_interval_usec: i64,
    /// Interval timer - current value seconds
    itimer_value_sec: i64,
    /// Interval timer - current value microseconds
    itimer_value_usec: i64,
}

impl Process {
    /// Create a new process (thread group leader)
    pub fn new(
        pid: Pid,
        ppid: Pid,
        address_space: UserAddressSpace,
        kernel_stack: PhysAddr,
        kernel_stack_size: usize,
        entry_point: VirtAddr,
        user_stack_top: VirtAddr,
    ) -> Self {
        Self {
            pid,
            tid: pid,  // TID == PID for thread group leader
            tgid: pid, // TGID == PID for thread group leader
            ppid,
            state: ProcessState::Ready,
            exit_status: 0,
            credentials: Credentials::default(),
            pgid: pid, // New process is its own process group leader
            sid: pid,  // New process starts a new session
            address_space,
            shared_address_space: None,
            context: ProcessContext::default(),
            kernel_stack,
            kernel_stack_size,
            user_stack_top,
            entry_point,
            children: Vec::new(),
            owned_frames: Vec::new(),
            fd_table: FdTable::new(),
            shared_fd_table: None,
            signal_mask: SigSet::empty(),
            pending_signals: PendingSignals::new(),
            sigactions: [SigAction::new(); NSIG],
            cwd: String::from("/"),
            cmdline: Vec::new(),
            environ: Vec::new(),
            tls: 0,
            clear_child_tid: 0,
            is_thread_leader: true,
            thread_group: Vec::new(),
            nice: 0, // Default priority
            alarm_remaining: 0,
            itimer_interval_sec: 0,
            itimer_interval_usec: 0,
            itimer_value_sec: 0,
            itimer_value_usec: 0,
        }
    }

    /// Create a new thread (shares resources with parent based on flags)
    pub fn new_thread(
        tid: Tid,
        tgid: Pid,
        ppid: Pid,
        kernel_stack: PhysAddr,
        kernel_stack_size: usize,
        entry_point: VirtAddr,
        user_stack_top: VirtAddr,
        shared_address_space: Arc<Mutex<UserAddressSpace>>,
        shared_fd_table: Option<Arc<Mutex<FdTable>>>,
        credentials: Credentials,
        pgid: Pid,
        sid: Pid,
        sigactions: [SigAction; NSIG],
        cwd: String,
    ) -> Self {
        // For threads, we use the shared address space's data
        let address_space = {
            let locked = shared_address_space.lock();
            // Create a minimal address space for the thread with the same PML4
            unsafe { UserAddressSpace::from_raw(locked.pml4_phys(), Vec::new()) }
        };

        Self {
            pid: tid as Pid, // Internal PID is unique per thread
            tid,
            tgid,
            ppid,
            state: ProcessState::Ready,
            exit_status: 0,
            credentials,
            pgid,
            sid,
            address_space,
            shared_address_space: Some(shared_address_space),
            context: ProcessContext::default(),
            kernel_stack,
            kernel_stack_size,
            user_stack_top,
            entry_point,
            children: Vec::new(),
            owned_frames: Vec::new(),
            fd_table: FdTable::new(),
            shared_fd_table,
            signal_mask: SigSet::empty(),
            pending_signals: PendingSignals::new(),
            sigactions,
            cwd,
            cmdline: Vec::new(),
            environ: Vec::new(),
            tls: 0,
            clear_child_tid: 0,
            is_thread_leader: false,
            thread_group: Vec::new(),
            nice: 0, // Default priority
            alarm_remaining: 0,
            itimer_interval_sec: 0,
            itimer_interval_usec: 0,
            itimer_value_sec: 0,
            itimer_value_usec: 0,
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

    /// Get the thread ID
    pub fn tid(&self) -> Tid {
        self.tid
    }

    /// Get the thread group ID (the "PID" seen by userspace)
    pub fn tgid(&self) -> Pid {
        self.tgid
    }

    /// Check if this is the thread group leader
    pub fn is_thread_leader(&self) -> bool {
        self.is_thread_leader
    }

    /// Get the TLS (thread-local storage) pointer
    pub fn tls(&self) -> u64 {
        self.tls
    }

    /// Set the TLS pointer
    pub fn set_tls(&mut self, tls: u64) {
        self.tls = tls;
    }

    /// Get the clear_child_tid address
    pub fn clear_child_tid(&self) -> u64 {
        self.clear_child_tid
    }

    /// Set the clear_child_tid address (for CLONE_CHILD_CLEARTID)
    pub fn set_clear_child_tid(&mut self, addr: u64) {
        self.clear_child_tid = addr;
    }

    /// Get the thread group members
    pub fn thread_group(&self) -> &[Tid] {
        &self.thread_group
    }

    /// Add a thread to the thread group (only valid for leader)
    pub fn add_thread(&mut self, tid: Tid) {
        if self.is_thread_leader {
            self.thread_group.push(tid);
        }
    }

    /// Remove a thread from the thread group
    pub fn remove_thread(&mut self, tid: Tid) {
        self.thread_group.retain(|&t| t != tid);
    }

    /// Get shared address space (for threads)
    pub fn shared_address_space(&self) -> Option<&Arc<Mutex<UserAddressSpace>>> {
        self.shared_address_space.as_ref()
    }

    /// Get shared file descriptor table (for threads)
    pub fn shared_fd_table(&self) -> Option<&Arc<Mutex<FdTable>>> {
        self.shared_fd_table.as_ref()
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
    pub fn send_signal(&mut self, sig: i32, info: Option<signal::SigInfo>) {
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

    /// Get the command line arguments
    pub fn cmdline(&self) -> &[String] {
        &self.cmdline
    }

    /// Set the command line arguments
    pub fn set_cmdline(&mut self, cmdline: Vec<String>) {
        self.cmdline = cmdline;
    }

    /// Get the environment variables
    pub fn environ(&self) -> &[String] {
        &self.environ
    }

    /// Set the environment variables
    pub fn set_environ(&mut self, environ: Vec<String>) {
        self.environ = environ;
    }

    /// Clone cmdline (for fork)
    pub fn clone_cmdline(&self) -> Vec<String> {
        self.cmdline.clone()
    }

    /// Clone environ (for fork)
    pub fn clone_environ(&self) -> Vec<String> {
        self.environ.clone()
    }

    /// Get nice value (process priority)
    pub fn nice(&self) -> i32 {
        self.nice
    }

    /// Set nice value (process priority)
    pub fn set_nice(&mut self, nice: i32) {
        self.nice = nice;
    }

    /// Get alarm remaining time
    pub fn get_alarm_remaining(&self) -> u32 {
        self.alarm_remaining
    }

    /// Set alarm
    pub fn set_alarm(&mut self, seconds: u32) {
        self.alarm_remaining = seconds;
    }

    /// Clear alarm
    pub fn clear_alarm(&mut self) {
        self.alarm_remaining = 0;
    }

    /// Get interval timer interval seconds
    pub fn get_itimer_interval_sec(&self) -> i64 {
        self.itimer_interval_sec
    }

    /// Get interval timer interval microseconds
    pub fn get_itimer_interval_usec(&self) -> i64 {
        self.itimer_interval_usec
    }

    /// Get interval timer value seconds
    pub fn get_itimer_value_sec(&self) -> i64 {
        self.itimer_value_sec
    }

    /// Get interval timer value microseconds
    pub fn get_itimer_value_usec(&self) -> i64 {
        self.itimer_value_usec
    }

    /// Set interval timer
    pub fn set_itimer(
        &mut self,
        interval_sec: i64,
        interval_usec: i64,
        value_sec: i64,
        value_usec: i64,
    ) {
        self.itimer_interval_sec = interval_sec;
        self.itimer_interval_usec = interval_usec;
        self.itimer_value_sec = value_sec;
        self.itimer_value_usec = value_usec;
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
        let arc = Arc::new(Mutex::new(process));
        self.processes.write().insert(pid, Arc::clone(&arc));
        arc
    }

    /// Get a process by PID
    pub fn get(&self, pid: Pid) -> Option<Arc<Mutex<Process>>> {
        self.processes.read().get(&pid).cloned()
    }

    /// Remove a process from the table
    pub fn remove(&self, pid: Pid) -> Option<Arc<Mutex<Process>>> {
        self.processes.write().remove(&pid)
    }

    /// Get the current process PID
    pub fn current_pid(&self) -> Pid {
        self.current_pid.load(Ordering::SeqCst)
    }

    /// Set the current process PID
    pub fn set_current_pid(&self, pid: Pid) {
        self.current_pid.store(pid, Ordering::SeqCst);
    }

    /// Get the current process
    pub fn current(&self) -> Option<Arc<Mutex<Process>>> {
        let pid = self.current_pid();
        if pid == 0 { None } else { self.get(pid) }
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
