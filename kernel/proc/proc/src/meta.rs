//! Process metadata shared between threads
//!
//! ProcessMeta contains all process-level state that is shared between
//! threads in the same process. This includes file descriptors, credentials,
//! signal handlers, address space, and other process-wide resources.
//!
//! Multiple Tasks (threads) can share the same Arc<Mutex<ProcessMeta>>
//! when created with CLONE_VM | CLONE_FILES.

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use os_core::PhysAddr;
use sched_traits::Pid;
use signal::{NSIG, PendingSignals, SigAction, SigSet};
use spin::Mutex;
use vfs::FdTable;

use crate::UserAddressSpace;
use crate::process::Credentials;

/// Thread ID type (same as Pid internally but semantically different)
pub type Tid = u32;

/// Process metadata shared between threads
///
/// This struct holds all process-level state that is shared between threads
/// in the same process (thread group). Each Task holds an Arc<Mutex<ProcessMeta>>
/// to this structure.
///
/// For single-threaded processes, there is one Task and one ProcessMeta.
/// For multi-threaded processes, all Tasks in the thread group share the
/// same ProcessMeta.
pub struct ProcessMeta {
    /// Thread Group ID (the "PID" seen by userspace)
    /// All threads in a process share the same tgid
    pub tgid: Pid,

    /// Process group ID
    pub pgid: Pid,

    /// Session ID
    pub sid: Pid,

    /// User credentials (uid, gid, euid, egid)
    pub credentials: Credentials,

    /// Virtual address space
    /// Shared by all threads via CLONE_VM
    pub address_space: UserAddressSpace,

    /// Shared address space reference (for tracking shared ownership)
    pub shared_address_space: Option<Arc<Mutex<UserAddressSpace>>>,

    /// File descriptor table
    /// May be shared via CLONE_FILES
    pub fd_table: FdTable,

    /// Shared fd table reference (for threads with CLONE_FILES)
    pub shared_fd_table: Option<Arc<Mutex<FdTable>>>,

    /// Signal mask (blocked signals)
    pub signal_mask: SigSet,

    /// Pending signals
    pub pending_signals: PendingSignals,

    /// Signal actions (handlers)
    /// Shared via CLONE_SIGHAND
    pub sigactions: [SigAction; NSIG],

    /// Current working directory
    pub cwd: String,

    /// Command line arguments (for /proc/[pid]/cmdline)
    pub cmdline: Vec<String>,

    /// Environment variables (for /proc/[pid]/environ)
    pub environ: Vec<String>,

    /// Thread-local storage pointer (for CLONE_SETTLS)
    pub tls: u64,

    /// Address to clear and futex wake on thread exit (for CLONE_CHILD_CLEARTID)
    pub clear_child_tid: u64,

    /// Physical frames owned by this process (for cleanup on exit)
    pub owned_frames: Vec<PhysAddr>,

    /// Alarm remaining time in seconds (0 = no alarm)
    pub alarm_remaining: u32,

    /// Interval timer - interval seconds
    pub itimer_interval_sec: i64,

    /// Interval timer - interval microseconds
    pub itimer_interval_usec: i64,

    /// Interval timer - current value seconds
    pub itimer_value_sec: i64,

    /// Interval timer - current value microseconds
    pub itimer_value_usec: i64,

    /// Is this the thread group leader's metadata?
    pub is_thread_leader: bool,

    /// Thread group members (TIDs of threads in this group)
    /// Only maintained in the leader's ProcessMeta
    pub thread_group: Vec<Tid>,

    /// File creation mask (umask)
    pub umask: u16,

    /// Program break (heap end address for brk/sbrk)
    /// 🔥 GraveShift: Classic UNIX heap management 🔥
    pub program_break: u64,

    /// Next mmap hint address (per-process mmap allocator)
    /// ⚡ GraveShift: Fixed - was global, now per-process to avoid mmap collisions
    pub next_mmap_addr: u64,

    /// CPU time accumulated (nanoseconds)
    /// Updated by scheduler on context switch
    pub cpu_time_ns: u64,

    /// Signal number that stopped this process (for waitpid WIFSTOPPED)
    /// None = not stopped, Some(sig) = stopped by this signal
    /// — ThreadRogue: freeze-frame state for job control
    pub stop_signal: Option<u8>,

    /// Process was continued via SIGCONT (for waitpid WIFCONTINUED)
    /// Reset to false after waitpid reports it
    /// — ThreadRogue: thaw notification for the parent
    pub continued: bool,

    /// Controlling terminal device number (major << 8 | minor)
    /// 0 means no controlling terminal
    /// — GraveShift: TTY hookup for interactive sessions
    pub tty_nr: u32,
}

impl ProcessMeta {
    /// Create new ProcessMeta for a process (thread group leader)
    pub fn new(tgid: Pid, address_space: UserAddressSpace) -> Self {
        Self {
            tgid,
            pgid: tgid, // New process is its own process group leader
            sid: tgid,  // New process starts a new session
            credentials: Credentials::default(),
            address_space,
            shared_address_space: None,
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
            owned_frames: Vec::new(),
            alarm_remaining: 0,
            itimer_interval_sec: 0,
            itimer_interval_usec: 0,
            itimer_value_sec: 0,
            itimer_value_usec: 0,
            is_thread_leader: true,
            thread_group: Vec::new(),
            umask: 0o022,
            program_break: 0x600000, // Initial heap start (after typical program load area)
            next_mmap_addr: 0x0000_7000_0000_0000, // Initial mmap hint address
            cpu_time_ns: 0,
            stop_signal: None,
            continued: false,
            tty_nr: 0, // No controlling terminal by default
        }
    }

    /// Create ProcessMeta for a kernel task (like idle)
    pub fn new_kernel() -> Self {
        // Create a minimal address space for kernel tasks
        let address_space = unsafe { UserAddressSpace::from_raw(PhysAddr::new(0), Vec::new()) };

        Self {
            tgid: 0,
            pgid: 0,
            sid: 0,
            credentials: Credentials::ROOT,
            address_space,
            shared_address_space: None,
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
            owned_frames: Vec::new(),
            alarm_remaining: 0,
            itimer_interval_sec: 0,
            itimer_interval_usec: 0,
            itimer_value_sec: 0,
            itimer_value_usec: 0,
            is_thread_leader: true,
            thread_group: Vec::new(),
            umask: 0o022,
            program_break: 0x600000, // Initial heap start (after typical program load area)
            next_mmap_addr: 0x0000_7000_0000_0000, // Initial mmap hint address
            cpu_time_ns: 0,
            stop_signal: None,
            continued: false,
            tty_nr: 0, // No controlling terminal by default
        }
    }

    /// Clone ProcessMeta for fork (creates independent copy)
    pub fn clone_for_fork(&self, new_tgid: Pid, new_address_space: UserAddressSpace) -> Self {
        Self {
            tgid: new_tgid,
            pgid: self.pgid, // Inherit process group
            sid: self.sid,   // Inherit session
            credentials: self.credentials,
            address_space: new_address_space,
            shared_address_space: None,
            fd_table: self.fd_table.clone_for_fork(),
            shared_fd_table: None,
            signal_mask: self.signal_mask.clone(),
            pending_signals: PendingSignals::new(), // Fresh pending signals
            sigactions: self.sigactions.clone(),
            cwd: self.cwd.clone(),
            cmdline: self.cmdline.clone(),
            environ: self.environ.clone(),
            tls: 0, // Reset TLS for child
            clear_child_tid: 0,
            owned_frames: Vec::new(), // Child doesn't own parent's frames
            alarm_remaining: 0,       // Alarms not inherited
            itimer_interval_sec: 0,
            itimer_interval_usec: 0,
            itimer_value_sec: 0,
            itimer_value_usec: 0,
            is_thread_leader: true,
            thread_group: Vec::new(),
            umask: self.umask,
            program_break: self.program_break, // Inherit parent's program break
            next_mmap_addr: self.next_mmap_addr, // Inherit parent's mmap hint
            cpu_time_ns: 0, // Child starts with 0 CPU time
            stop_signal: None,
            continued: false,
            tty_nr: self.tty_nr, // Inherit controlling terminal
        }
    }

    /// Get the PML4 physical address for address space switching
    pub fn pml4_phys(&self) -> PhysAddr {
        self.address_space.pml4_phys()
    }

    /// Check if there are any deliverable signals
    pub fn has_pending_signals(&self) -> bool {
        self.pending_signals.has_deliverable(&self.signal_mask)
    }

    /// Add a pending signal
    pub fn send_signal(&mut self, sig: i32, info: Option<signal::SigInfo>) {
        self.pending_signals.add(sig, info);
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

    /// Add a thread to the thread group
    pub fn add_thread(&mut self, tid: Tid) {
        if self.is_thread_leader {
            self.thread_group.push(tid);
        }
    }

    /// Remove a thread from the thread group
    pub fn remove_thread(&mut self, tid: Tid) {
        self.thread_group.retain(|&t| t != tid);
    }

    /// Add an owned frame (for cleanup on process exit)
    pub fn add_owned_frame(&mut self, frame: PhysAddr) {
        self.owned_frames.push(frame);
    }

    /// Take ownership of all owned frames (for cleanup)
    pub fn take_owned_frames(&mut self) -> Vec<PhysAddr> {
        core::mem::take(&mut self.owned_frames)
    }

    /// Get interval timer
    pub fn get_itimer(&self) -> (i64, i64, i64, i64) {
        (
            self.itimer_interval_sec,
            self.itimer_interval_usec,
            self.itimer_value_sec,
            self.itimer_value_usec,
        )
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
