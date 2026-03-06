//! Process metadata shared between threads
//!
//! ProcessMeta contains all process-level state that is shared between
//! threads in the same process. This includes file descriptors, credentials,
//! signal handlers, address space, and other process-wide resources.
//!
//! Multiple Tasks (threads) can share the same Arc<Mutex<ProcessMeta>>
//! when created with CLONE_VM | CLONE_FILES.
//!
//! — ColdCipher: This module also owns the initial ASLR jitter for mmap_base.
//! Every new process gets a randomly-seeded starting point so the first mmap(NULL)
//! call doesn't land at the same predictable address every single time.

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use mm_manager::try_mm;
use os_core::PhysAddr;
use sched_traits::Pid;
use signal::{NSIG, PendingSignals, SigAction, SigSet};
use spin::Mutex;
use vfs::FdTable;

use crate::UserAddressSpace;
use crate::process::Credentials;

/// Thread ID type (same as Pid internally but semantically different)
pub type Tid = u32;

/// Default mmap base address — canonical starting point before ASLR jitter.
///
/// — ColdCipher: Sits clear of the heap (0x600000) and the user stack (sub-0x8000_0000_0000).
/// Gives ~128TB of virtual address space for mappings. Plenty of room to get lost in.
const MMAP_BASE_DEFAULT: u64 = 0x0000_7000_0000_0000;

/// Maximum random downward shift applied to the initial mmap base.
///
/// — ColdCipher: 4MB page-aligned mask. Cheap to generate, meaningful to exploit.
/// An attacker who guesses a 1-in-1024 offset still has to contend with stack ASLR
/// and the absence of a crystal ball. Combined entropy is additive.
const ASLR_MMAP_INIT_ENTROPY: u64 = 0x3FF_F000; // 4MB - 4KB, page-aligned

/// Generate a random 64-bit value from TSC for ASLR entropy.
///
/// — ColdCipher: RDTSC is always available on x86_64 and the low bits
/// are sufficiently unpredictable for ASLR offsets. Called once per
/// process birth — not a hot path. We XOR high and low halves to
/// spread entropy across all 64 bits.
#[inline]
pub(crate) fn aslr_random() -> u64 {
    // — ColdCipher: routed through os_core arch abstraction — no more raw rdtsc
    // in process code. The low bits still have the most jitter; mix them.
    let tsc = os_core::read_tsc();
    tsc ^ tsc.rotate_right(17)
}

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

    /// Physical frames owned by this process (for cleanup on exit).
    ///
    /// Each entry is (base_phys, page_count). page_count must match the
    /// alloc_frames() / alloc_contiguous() call that allocated the region.
    /// — GraveShift: Size matters. free_frame(addr) only frees 4KB.
    /// A 128KB kernel stack needs free_frames(addr, 32). Store the count.
    pub owned_frames: Vec<(PhysAddr, usize)>,

    /// Guard page physical addresses (for kernel stack overflow detection).
    ///
    /// — BlackLatch: One guard frame sits below each kernel stack. Its PTE
    /// is cleared in the direct map so any overflow #PFs immediately.
    /// On process exit, each guard page must be RE-MAPPED before freeing —
    /// the buddy allocator writes canaries into freed frames and will triple-
    /// fault if it touches a not-present page. Remap first, free second.
    pub guard_pages: Vec<PhysAddr>,

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
            guard_pages: Vec::new(),
            alarm_remaining: 0,
            itimer_interval_sec: 0,
            itimer_interval_usec: 0,
            itimer_value_sec: 0,
            itimer_value_usec: 0,
            is_thread_leader: true,
            thread_group: Vec::new(),
            umask: 0o022,
            program_break: 0x600000, // Initial heap start (after typical program load area)
            // — ColdCipher: Randomize the initial mmap base so even before the
            // first exec() the process address layout is non-deterministic.
            // Each new process (clone/fork path excluded) starts from a different
            // region. exec() will overwrite this with its own ASLR jitter anyway.
            next_mmap_addr: MMAP_BASE_DEFAULT
                - (aslr_random() & ASLR_MMAP_INIT_ENTROPY),
            cpu_time_ns: 0,
            stop_signal: None,
            continued: false,
            tty_nr: 0, // No controlling terminal by default
        }
    }

    /// Create ProcessMeta for a kernel task (like idle)
    pub fn new_kernel() -> Self {
        // Create a minimal address space for kernel tasks
        let address_space = unsafe { UserAddressSpace::from_raw(PhysAddr::new(0), Vec::new(), mm_vma::VmAreaList::new()) };

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
            guard_pages: Vec::new(),
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
            guard_pages: Vec::new(),  // Guard pages assigned after fork in kernel crate
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
            cpu_time_ns: 0,                    // Child starts with 0 CPU time
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

    /// Add an owned contiguous frame range (for cleanup on process exit).
    ///
    /// `pages` is the number of 4KB pages in the contiguous allocation.
    /// Must match the alloc_frames(pages) / alloc_contiguous(pages) call
    /// that produced `frame`. The Drop impl calls free_frames(frame, pages).
    /// — GraveShift: If you forget to pass the right count, you'll leak or corrupt.
    pub fn add_owned_frames(&mut self, frame: PhysAddr, pages: usize) {
        self.owned_frames.push((frame, pages));
    }

    /// Register a guard page physical address.
    ///
    /// — BlackLatch: The guard frame's PTE has been cleared from the kernel
    /// direct map. On process exit (Drop), we remap it before freeing so the
    /// buddy allocator can write its canaries without #PF-ing.
    ///
    /// The guard frame itself is tracked in owned_frames (as part of the
    /// full stack allocation). This vec just holds the addresses that need
    /// a PTE restore before the buddy allocator touches them.
    pub fn add_guard_page(&mut self, guard_phys: PhysAddr) {
        self.guard_pages.push(guard_phys);
    }

    /// Take ownership of all owned frames (for cleanup)
    pub fn take_owned_frames(&mut self) -> Vec<(PhysAddr, usize)> {
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

/// Drop implementation for ProcessMeta — frees kernel-owned frames on exit.
///
/// — GraveShift: Kernel stacks, DMA buffers, and any other frames registered
/// via add_owned_frames() are freed here. The address_space field (UserAddressSpace)
/// handles user data + page table frames via its own Drop impl. This deals with
/// the kernel-side bookkeeping (stack pages, etc.) that live outside the page tables.
///
/// owned_frames is populated by do_fork() for the kernel stack allocation and by
/// any other subsystem that registers process-owned physical frames.
impl Drop for ProcessMeta {
    fn drop(&mut self) {
        // — GraveShift: No mm = early boot. owned_frames are probably empty
        // for kernel pseudo-tasks anyway. Skip silently.
        let mm = match try_mm() {
            Some(m) => m,
            None => return,
        };

        if self.owned_frames.is_empty() && self.guard_pages.is_empty() {
            return;
        }

        // — BlackLatch: CRITICAL ORDER — remap guard pages BEFORE freeing
        // any frames. The buddy allocator writes canary values into freed
        // frames immediately. If a guard page frame is in the free list
        // while its PTE is still cleared, the next write to that frame
        // will #PF in the buddy allocator. Remap first, THEN free. Always.
        //
        // We use the current CR3 to reach the kernel direct map page tables.
        // Because all PML4s share the same physical kernel-half page tables
        // (entries 256-511 point to the same PDPT/PD/PT frames), restoring
        // a PTE via the current CR3 restores it globally. No per-CPU work needed.
        for &guard_phys in &self.guard_pages {
            if guard_phys.as_u64() == 0 {
                continue; // — BlackLatch: Null guard? Skip, don't fault on it.
            }
            // Remap the guard page in the kernel direct map.
            // Safety: guard_phys was allocated and its PTE was cleared by
            // kstack_guard::unmap_guard_page(). We are the sole owner.
            unsafe {
                remap_guard_page_in_drop(guard_phys);
            }
        }

        #[cfg(feature = "debug-proc")]
        let count = self.owned_frames.len() as u32;

        // — GraveShift: owned_frames holds kernel-side frames (e.g. 128KB kernel stack).
        // These are never COW-shared — just free them all unconditionally.
        // Each entry is (base_addr, page_count). We use free_contiguous() so the
        // buddy allocator gets the correct order (not a 4KB stub of a 128KB block).
        for &(frame, pages) in &self.owned_frames {
            if frame.as_u64() == 0 || pages == 0 {
                // — GraveShift: Zero address or zero count = garbage. Skip.
                continue;
            }
            let _ = mm.free_contiguous(frame, pages);
        }

        #[cfg(feature = "debug-proc")]
        unsafe {
            os_log::write_str_raw("[META-DROP] tgid=");
            os_log::write_u32_raw(self.tgid);
            os_log::write_str_raw(" freed_owned=");
            os_log::write_u32_raw(count);
            os_log::write_str_raw("\n");
        }
    }
}

/// Restore a guard page PTE in the kernel direct map before freeing.
///
/// — BlackLatch: Duplicated from kstack_guard to avoid a circular crate dep.
/// The proc crate can't import the kernel crate. We inline the minimal PTE
/// write here: walk current CR3, find the 4KB PTE for phys_to_virt(guard_phys),
/// set PRESENT | WRITABLE | NO_EXECUTE | GLOBAL, then shootdown.
///
/// If the PTE is not found (huge page or missing table), log and skip.
/// The worst case is a buddy allocator fault, which is better than silently
/// leaking memory.
///
/// # Safety
/// * `guard_phys` must be the address passed to unmap_guard_page().
/// * Must be called before free_contiguous() for the owning allocation.
unsafe fn remap_guard_page_in_drop(guard_phys: PhysAddr) {
    use mm_paging::{PageTable, PageTableFlags, PHYS_MAP_BASE, phys_to_virt, read_cr3};

    let guard_virt_addr = guard_phys.as_u64() + PHYS_MAP_BASE;
    let addr = guard_virt_addr;

    let pml4_idx = ((addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((addr >> 12) & 0x1FF) as usize;

    let pml4_phys = read_cr3();
    let pml4 = unsafe { &mut *phys_to_virt(pml4_phys).as_mut_ptr::<PageTable>() };
    let e0 = &pml4[pml4_idx];
    if !e0.is_present() {
        return;
    }

    let pdpt = unsafe { &mut *phys_to_virt(e0.addr()).as_mut_ptr::<PageTable>() };
    let e1 = &pdpt[pdpt_idx];
    if !e1.is_present() || e1.is_huge() {
        return;
    }

    let pd = unsafe { &mut *phys_to_virt(e1.addr()).as_mut_ptr::<PageTable>() };
    let e2 = &pd[pd_idx];
    if !e2.is_present() || e2.is_huge() {
        return;
    }

    let pt = unsafe { &mut *phys_to_virt(e2.addr()).as_mut_ptr::<PageTable>() };
    let entry = &mut pt[pt_idx];

    // — BlackLatch: Restore the PTE so the buddy allocator can write here.
    entry.set(
        guard_phys,
        PageTableFlags::PRESENT
            | PageTableFlags::WRITABLE
            | PageTableFlags::NO_EXECUTE
            | PageTableFlags::GLOBAL,
    );

    // TLB shootdown — all CPUs must see the restored mapping.
    smp::tlb_shootdown(guard_virt_addr, guard_virt_addr + 4096, 0);
}
