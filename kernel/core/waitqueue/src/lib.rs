//! Generic wait queues for OXIDE OS
//!
//! — SableWire: Linux's wait_queue_head_t is the backbone of every blocking
//! operation in the kernel. Our version is simpler — fixed-capacity atomic
//! PID slots, no spinlock, no linked list. Trade-off: max 16 waiters per
//! queue. If you need more, you're doing something wrong (or writing a
//! web server in kernel space, in which case, stop).
//!
//! Zero heap allocation. ISR-safe wake. Lock-free register/unregister.

#![no_std]

use core::sync::atomic::{AtomicU32, Ordering};

/// — SableWire: u32::MAX is never a valid PID. The scheduler caps PIDs well
/// below this. Using it as "empty slot" means we never need Option<u32>.
const PID_NONE: u32 = u32::MAX;

/// Maximum waiters per queue. 16 covers pipes (2 ends × a few processes),
/// VTs (one reader per terminal), and sockets (connection backlog).
/// — SableWire: If you're hitting this limit, you have a design problem,
/// not a capacity problem.
const MAX_WAITERS: usize = 16;

/// Fixed-capacity, lock-free wait queue.
///
/// Processes register their PID when they want to be woken on an event.
/// Drivers call `wake_one()` or `wake_all()` when the event occurs.
/// The wake callback is set once at boot via `set_wake_fn()`.
///
/// — SableWire: No Mutex, no heap, no linked list. Just atoms and contempt
/// for complexity. Safe to call from ISR context (wake path is non-blocking).
pub struct WaitQueue {
    slots: [AtomicU32; MAX_WAITERS],
    len: AtomicU32,
}

// — SableWire: The compiler needs convincing that AtomicU32 arrays are Send+Sync.
// They are — atomics are the definition of thread-safe.
unsafe impl Send for WaitQueue {}
unsafe impl Sync for WaitQueue {}

/// — SableWire: Global wake callback. Set once at boot, never changes.
/// Uses extern "Rust" fn because we can't depend on the sched crate
/// (circular dependency hell). The kernel registers the real wake function
/// during init.
static WAKE_FN: AtomicFnPtr = AtomicFnPtr::new();

/// Atomic function pointer wrapper. AtomicU64 holding a fn pointer.
/// — SableWire: Yes, this is a raw u64 cast. No, there's no better way
/// in no_std without alloc. The pointer is set once and never changes.
struct AtomicFnPtr {
    ptr: AtomicU64,
}

use core::sync::atomic::AtomicU64;

impl AtomicFnPtr {
    const fn new() -> Self {
        Self { ptr: AtomicU64::new(0) }
    }

    fn set(&self, f: fn(u32)) {
        self.ptr.store(f as usize as u64, Ordering::Release);
    }

    fn get(&self) -> Option<fn(u32)> {
        let p = self.ptr.load(Ordering::Acquire);
        if p == 0 {
            None
        } else {
            // — SableWire: Safe because we only store valid fn pointers via set().
            Some(unsafe { core::mem::transmute::<u64, fn(u32)>(p) })
        }
    }
}

// — SableWire: AtomicU64 is Send+Sync. The fn pointer it holds is a static
// function — no captured state, no lifetime issues.
unsafe impl Send for AtomicFnPtr {}
unsafe impl Sync for AtomicFnPtr {}

/// Register the scheduler wake function. Called once during kernel init.
/// — SableWire: This breaks the circular dependency between waitqueue and sched.
/// The sched crate provides try_wake_up(pid), we store it as a function pointer.
pub fn set_wake_fn(f: fn(u32)) {
    WAKE_FN.set(f);
}

/// Wake a single PID via the registered callback.
fn do_wake(pid: u32) {
    if let Some(f) = WAKE_FN.get() {
        f(pid);
    }
}

impl WaitQueue {
    /// Create a new empty wait queue. All slots initialized to PID_NONE.
    /// — SableWire: const fn so these can live in statics. No init() dance.
    pub const fn new() -> Self {
        // — SableWire: Can't use a loop in const fn (yet). Macro-free manual init.
        // 16 slots of AtomicU32::new(PID_NONE). Yes, it's ugly. No, I don't care.
        Self {
            slots: [
                AtomicU32::new(PID_NONE), AtomicU32::new(PID_NONE),
                AtomicU32::new(PID_NONE), AtomicU32::new(PID_NONE),
                AtomicU32::new(PID_NONE), AtomicU32::new(PID_NONE),
                AtomicU32::new(PID_NONE), AtomicU32::new(PID_NONE),
                AtomicU32::new(PID_NONE), AtomicU32::new(PID_NONE),
                AtomicU32::new(PID_NONE), AtomicU32::new(PID_NONE),
                AtomicU32::new(PID_NONE), AtomicU32::new(PID_NONE),
                AtomicU32::new(PID_NONE), AtomicU32::new(PID_NONE),
            ],
            len: AtomicU32::new(0),
        }
    }

    /// Register a PID as a waiter. Returns the slot index on success,
    /// None if the queue is full.
    /// — SableWire: Lock-free CAS loop. Multiple CPUs can register concurrently.
    pub fn register(&self, pid: u32) -> Option<usize> {
        for (i, slot) in self.slots.iter().enumerate() {
            if slot.compare_exchange(
                PID_NONE, pid,
                Ordering::AcqRel, Ordering::Relaxed,
            ).is_ok() {
                self.len.fetch_add(1, Ordering::Release);
                return Some(i);
            }
        }
        None
    }

    /// Unregister a waiter by slot index.
    /// — SableWire: O(1). The caller knows their slot from register().
    pub fn unregister(&self, slot: usize) {
        if slot < MAX_WAITERS {
            let old = self.slots[slot].swap(PID_NONE, Ordering::AcqRel);
            if old != PID_NONE {
                self.len.fetch_sub(1, Ordering::Release);
            }
        }
    }

    /// Unregister a waiter by PID (for cleanup on exit/signal).
    /// — SableWire: O(N) scan, but N=16 and this is the error path, not hot.
    pub fn unregister_pid(&self, pid: u32) {
        for slot in self.slots.iter() {
            if slot.compare_exchange(
                pid, PID_NONE,
                Ordering::AcqRel, Ordering::Relaxed,
            ).is_ok() {
                self.len.fetch_sub(1, Ordering::Release);
                return;
            }
        }
    }

    /// Wake all registered waiters. Each woken PID is atomically removed.
    /// — SableWire: ISR-safe. No locks, no alloc. Just swap-and-wake.
    /// Called from pipe write, VT input push, socket receive, etc.
    pub fn wake_all(&self) {
        for slot in self.slots.iter() {
            let pid = slot.swap(PID_NONE, Ordering::AcqRel);
            if pid != PID_NONE {
                self.len.fetch_sub(1, Ordering::Release);
                do_wake(pid);
            }
        }
    }

    /// Wake the first registered waiter. Returns true if someone was woken.
    /// — SableWire: For single-consumer patterns (e.g., accept() on a listen socket).
    pub fn wake_one(&self) -> bool {
        for slot in self.slots.iter() {
            let pid = slot.swap(PID_NONE, Ordering::AcqRel);
            if pid != PID_NONE {
                self.len.fetch_sub(1, Ordering::Release);
                do_wake(pid);
                return true;
            }
        }
        false
    }

    /// Check if anyone is waiting. Fast path for drivers that want to skip
    /// wake overhead when nobody cares.
    /// — SableWire: Relaxed ordering is fine — stale reads just mean we
    /// do one extra wake_all() on an empty queue. Not a correctness issue.
    pub fn has_waiters(&self) -> bool {
        self.len.load(Ordering::Relaxed) > 0
    }
}

/// Stack-allocated poll registration table.
///
/// Each sys_poll/sys_select call creates one on the stack. It tracks which
/// WaitQueues the caller registered on, so we can bulk-unregister on return.
///
/// — SableWire: Linux's poll_table does the same thing but with linked lists
/// and kmalloc. We use a fixed array because poll() on >32 fds is a sin.
pub struct PollTable {
    entries: [PollEntry; MAX_POLL_ENTRIES],
    count: usize,
    pid: u32,
}

/// Maximum number of wait queues a single poll/select can register on.
/// — SableWire: 32 covers even the most ambitious select() call. Each fd
/// may register on at most one WaitQueue (read OR write, not both).
const MAX_POLL_ENTRIES: usize = 32;

/// A single registration in the PollTable.
#[derive(Clone, Copy)]
struct PollEntry {
    /// Raw pointer to the WaitQueue. Valid for the duration of the syscall
    /// because WaitQueues live in static or Arc-protected structures.
    wq: *const WaitQueue,
    /// Slot index returned by WaitQueue::register()
    slot: usize,
}

impl Default for PollEntry {
    fn default() -> Self {
        Self {
            wq: core::ptr::null(),
            slot: 0,
        }
    }
}

impl PollTable {
    /// Create a new PollTable for the given PID.
    pub fn new(pid: u32) -> Self {
        Self {
            entries: [PollEntry::default(); MAX_POLL_ENTRIES],
            count: 0,
            pid,
        }
    }

    /// Register on a WaitQueue. Called by VnodeOps::poll_register_wait().
    /// — SableWire: The raw pointer is safe because WaitQueues outlive the
    /// syscall (they're embedded in pipes, VTs, sockets — all Arc or static).
    pub fn register(&mut self, wq: &WaitQueue) {
        if self.count >= MAX_POLL_ENTRIES {
            return; // — SableWire: silently drop. Better than panicking in a syscall.
        }
        if let Some(slot) = wq.register(self.pid) {
            self.entries[self.count] = PollEntry {
                wq: wq as *const WaitQueue,
                slot,
            };
            self.count += 1;
        }
    }

    /// Unregister from all WaitQueues. Called on syscall return, timeout, or signal.
    /// — SableWire: Must be called before returning from poll/select. Leaking
    /// registrations means stale PIDs in WaitQueues → waking dead processes.
    pub fn unregister_all(&mut self) {
        for i in 0..self.count {
            let entry = &self.entries[i];
            if !entry.wq.is_null() {
                // — SableWire: Safe because the WaitQueue outlives the syscall.
                // We're unwinding our own registrations, not touching foreign state.
                unsafe {
                    (*entry.wq).unregister(entry.slot);
                }
            }
        }
        self.count = 0;
    }

    /// Get the PID this table is registered for.
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

impl Drop for PollTable {
    fn drop(&mut self) {
        // — SableWire: Safety net. If someone forgets to call unregister_all(),
        // we clean up on drop. Defense in depth, not laziness.
        self.unregister_all();
    }
}
