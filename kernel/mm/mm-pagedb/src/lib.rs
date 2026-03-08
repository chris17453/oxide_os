//! Linux-style Page Frame Database (struct page) for OXIDE
//!
//! — GraveShift: Every physical frame in the system gets a 16-byte metadata entry.
//! No exceptions. No excuses. The old regime of "trust the canary" and "hope nobody
//! double-frees" is over. Every alloc, free, COW share, and PT allocation goes through
//! here. Corruption gets caught at the point of error, not three faults downstream.
//!
//! Cost: 16 bytes × 65536 frames (256MB) = 1MB. Cheap insurance against the
//! memory corruption hellscape we've been living in.

#![no_std]

use core::sync::atomic::{AtomicPtr, AtomicU32, AtomicU8, AtomicUsize, Ordering};
use os_core::PhysAddr;

// ============================================================================
// Event ring buffer — GraveShift: Record the last N alloc/free events so we
// can dump the history when corruption is detected. No serial spam during
// normal operation. When a DoubleFree or RefcountUnderflow occurs, we dump
// the last 64 events to show exactly what happened leading up to it.
// ============================================================================

/// Event types for the ring buffer
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum EventType {
    Alloc = 0,
    Free = 1,
    MarkPT = 2,
    RefInc = 3,
    RefDec = 4,
    CowClaim = 5,
}

/// A single alloc/free event
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageEvent {
    /// Physical address (frame number * 4096)
    pub phys: u64,
    /// Event type
    pub event: u8,
    /// Caller context (CTX_* constants)
    pub ctx: u8,
    /// Old flags before the operation
    pub old_flags: u8,
    /// Old refcount before the operation
    pub old_rc: u8,
    /// Owner PID at time of event
    pub owner: u32,
}

const EVENT_RING_SIZE: usize = 4096;

/// — GraveShift: Lock-free ring buffer. Races are acceptable — this is
/// diagnostic, not transactional. A torn read is infinitely better than
/// a deadlock in the allocator's hot path. Atomic index, raw writes.
static mut EVENT_RING: [PageEvent; EVENT_RING_SIZE] = [PageEvent {
    phys: 0,
    event: 0,
    ctx: 0,
    old_flags: 0,
    old_rc: 0,
    owner: 0,
}; EVENT_RING_SIZE];
static EVENT_RING_IDX: AtomicUsize = AtomicUsize::new(0);

/// Record an event in the ring buffer — lock-free, ISR-safe
#[inline]
fn record_event(phys: u64, event: EventType, ctx: u8, old_flags: u32, old_rc: u32, owner: u32) {
    let idx = EVENT_RING_IDX.fetch_add(1, Ordering::Relaxed) % EVENT_RING_SIZE;
    // Safety: idx is bounded by modulo. Torn writes are acceptable for diagnostics.
    unsafe {
        let entry = &mut EVENT_RING[idx];
        entry.phys = phys;
        entry.event = event as u8;
        entry.ctx = ctx;
        entry.old_flags = old_flags as u8;
        entry.old_rc = old_rc as u8;
        entry.owner = owner;
    }
}

static RING_DUMPED: AtomicU8 = AtomicU8::new(0);

/// Dump the event ring buffer for a specific frame — called on FIRST error only
/// Shows ALL events for the faulting frame to trace its full lifecycle.
pub fn dump_event_ring_for(target_phys: u64) {
    // — GraveShift: Only dump once to avoid serial saturation on cascading errors
    if RING_DUMPED.swap(1, Ordering::Relaxed) != 0 {
        return;
    }
    unsafe {
        os_log::write_str_raw("\n[PDB-RING] === History for 0x");
        os_log::write_u64_hex_raw(target_phys);
        os_log::write_str_raw(" ===\n");
    }
    let current = EVENT_RING_IDX.load(Ordering::Relaxed);
    let start = if current >= EVENT_RING_SIZE { current - EVENT_RING_SIZE } else { 0 };
    let end = current;
    let mut found = 0u32;
    for i in start..end {
        let idx = i % EVENT_RING_SIZE;
        let e = unsafe { &EVENT_RING[idx] };
        if e.phys != target_phys {
            continue;
        }
        found += 1;
        let event_name = match e.event {
            0 => "ALLOC",
            1 => "FREE ",
            2 => "MK-PT",
            3 => "R-INC",
            4 => "R-DEC",
            5 => "CLAIM",
            _ => "?????",
        };
        unsafe {
            os_log::write_str_raw("[PDB-RING] ");
            os_log::write_str_raw(event_name);
            os_log::write_str_raw(" 0x");
            os_log::write_u64_hex_raw(e.phys);
            os_log::write_str_raw(" flags=0x");
            os_log::write_u32_raw(e.old_flags as u32);
            os_log::write_str_raw(" rc=");
            os_log::write_u32_raw(e.old_rc as u32);
            os_log::write_str_raw(" pid=");
            os_log::write_u32_raw(e.owner);
            os_log::write_str_raw(" caller=");
            os_log::write_str_raw(context_name(e.ctx));
            os_log::write_str_raw("\n");
        }
    }
    if found == 0 {
        unsafe {
            os_log::write_str_raw("[PDB-RING] (no events found — ring wrapped past this frame)\n");
        }
    }
    unsafe {
        os_log::write_str_raw("[PDB-RING] === end (");
        os_log::write_u32_raw(found);
        os_log::write_str_raw(" events) ===\n\n");
    }

    // — GraveShift: Also dump the last 32 events regardless of frame,
    // to show the context of what was happening when the error occurred.
    unsafe {
        os_log::write_str_raw("[PDB-RING] === Last 32 events (all frames) ===\n");
    }
    let context_start = if current >= 32 { current - 32 } else { 0 };
    for i in context_start..current {
        let idx = i % EVENT_RING_SIZE;
        let e = unsafe { &EVENT_RING[idx] };
        if e.phys == 0 { continue; }
        let event_name = match e.event {
            0 => "ALLOC",
            1 => "FREE ",
            2 => "MK-PT",
            3 => "R-INC",
            4 => "R-DEC",
            5 => "CLAIM",
            _ => "?????",
        };
        unsafe {
            os_log::write_str_raw("[PDB-RING] ");
            os_log::write_str_raw(event_name);
            os_log::write_str_raw(" 0x");
            os_log::write_u64_hex_raw(e.phys);
            os_log::write_str_raw(" flags=0x");
            os_log::write_u32_raw(e.old_flags as u32);
            os_log::write_str_raw(" rc=");
            os_log::write_u32_raw(e.old_rc as u32);
            os_log::write_str_raw(" pid=");
            os_log::write_u32_raw(e.owner);
            os_log::write_str_raw(" caller=");
            os_log::write_str_raw(context_name(e.ctx));
            os_log::write_str_raw("\n");
        }
    }
    unsafe {
        os_log::write_str_raw("[PDB-RING] === end context ===\n\n");
    }
}

// ============================================================================
// Free context tracking — BlackLatch: WHO is calling free? Without this,
// a DoubleFree error tells you WHAT happened but not WHERE. Set the context
// before calling free_frame, read it in validate_free. Lock-free, ISR-safe.
// ============================================================================

/// Caller context for free operations (set before free, read in validate_free)
pub const CTX_UNKNOWN: u8 = 0;
pub const CTX_DROP_LEAF: u8 = 1;      // UserAddressSpace::Drop Step 1 — leaf data frame
pub const CTX_DROP_PT_WALK: u8 = 2;   // UserAddressSpace::Drop Step 2 — walked PT structure frame
pub const CTX_DROP_PML4: u8 = 3;      // UserAddressSpace::Drop Step 3 — PML4 frame
pub const CTX_DROP_ALLOC: u8 = 4;     // UserAddressSpace::Drop Step 4 — leftover allocated_frames
pub const CTX_MUNMAP: u8 = 5;         // sys_munmap
pub const CTX_BRK_SHRINK: u8 = 6;     // sys_brk shrink
pub const CTX_COW_FAULT: u8 = 7;      // COW fault handler
pub const CTX_BUDDY_COALESCE: u8 = 8; // Buddy allocator internal coalescing
pub const CTX_EXEC_CLEANUP: u8 = 9;   // exec old address space cleanup

static FREE_CONTEXT: AtomicU8 = AtomicU8::new(CTX_UNKNOWN);

/// Set the caller context before a free_frame call
#[inline]
pub fn set_free_context(ctx: u8) {
    FREE_CONTEXT.store(ctx, Ordering::Relaxed);
}

/// Get and print the current free context name
fn context_name(ctx: u8) -> &'static str {
    match ctx {
        CTX_DROP_LEAF => "Drop-leaf",
        CTX_DROP_PT_WALK => "Drop-pt-walk",
        CTX_DROP_PML4 => "Drop-pml4",
        CTX_DROP_ALLOC => "Drop-alloc-frames",
        CTX_MUNMAP => "munmap",
        CTX_BRK_SHRINK => "brk-shrink",
        CTX_COW_FAULT => "cow-fault",
        CTX_BUDDY_COALESCE => "buddy-coalesce",
        CTX_EXEC_CLEANUP => "exec-cleanup",
        _ => "unknown",
    }
}

// ============================================================================
// Page state flags — SableWire: Frame state machine. Every frame is exactly
// ONE primary state (Free/Allocated/Mapped/PageTable/Reserved) plus modifier
// flags that can be combined. If you're reading this comment, something already
// went wrong and you're debugging memory corruption. Good luck.
// ============================================================================

/// Frame is in the buddy free list (default state = 0)
pub const PF_FREE: u32 = 0;
/// Frame allocated from buddy, not yet mapped into any address space
pub const PF_ALLOCATED: u32 = 1 << 0;
/// Frame mapped into a user process page table (user data page)
pub const PF_MAPPED: u32 = 1 << 1;
/// Frame used as a page table structure (PML4/PDPT/PD/PT)
pub const PF_PAGETABLE: u32 = 1 << 2;
/// Boot-reserved, MMIO, kernel code — never allocatable
pub const PF_RESERVED: u32 = 1 << 3;
/// Copy-on-write shared (refcount > 1)
pub const PF_COW: u32 = 1 << 4;
/// Kernel-owned (heap, stacks, etc.)
pub const PF_KERNEL: u32 = 1 << 5;
/// Part of a slab cache (future)
pub const PF_SLAB: u32 = 1 << 6;
/// Modified since last writeback (future)
pub const PF_DIRTY: u32 = 1 << 7;

/// Frame size constant
const FRAME_SIZE: u64 = 4096;
/// Frame shift (log2(FRAME_SIZE))
const FRAME_SHIFT: u32 = 12;

// ============================================================================
// PageFrame — 16 bytes per frame. Atomic everything because SMP exists and
// ISRs don't care about your locking strategy. — TorqueJax
// ============================================================================

/// Per-frame metadata entry. Linux calls this `struct page`.
///
/// — GraveShift: 16 bytes × 65536 frames (256MB) = 1MB overhead. Every frame
/// in the system gets one. Indexed by PFN (physical frame number). All fields
/// are atomic because the scheduler doesn't ask permission before preempting you.
#[repr(C)]
pub struct PageFrame {
    /// Atomic flags + state (PF_* constants)
    flags: AtomicU32,
    /// Reference count. 0 = free. 1 = exclusive owner. >1 = shared (COW).
    refcount: AtomicU32,
    /// Owner PID (0 = kernel/unowned, u32::MAX = boot/reserved)
    owner: AtomicU32,
    /// Upper 8 bits = buddy order for head of compound block. Lower 24 = reserved.
    order_and_reserved: AtomicU32,
}

impl PageFrame {
    /// Create a zeroed PageFrame (free state)
    const fn new() -> Self {
        Self {
            flags: AtomicU32::new(PF_FREE),
            refcount: AtomicU32::new(0),
            owner: AtomicU32::new(0),
            order_and_reserved: AtomicU32::new(0),
        }
    }

    /// Get current flags
    #[inline]
    pub fn flags(&self) -> u32 {
        self.flags.load(Ordering::Relaxed)
    }

    /// Get current refcount
    #[inline]
    pub fn refcount(&self) -> u32 {
        self.refcount.load(Ordering::Relaxed)
    }

    /// Get owner PID
    #[inline]
    pub fn owner(&self) -> u32 {
        self.owner.load(Ordering::Relaxed)
    }

    /// Set flags (replaces all flags)
    #[inline]
    pub fn set_flags(&self, flags: u32) {
        self.flags.store(flags, Ordering::Relaxed);
    }

    /// Add a flag (OR into existing)
    #[inline]
    pub fn set_flag(&self, flag: u32) {
        self.flags.fetch_or(flag, Ordering::Relaxed);
    }

    /// Clear a flag
    #[inline]
    pub fn clear_flag(&self, flag: u32) {
        self.flags.fetch_and(!flag, Ordering::Relaxed);
    }

    /// Test if a flag is set
    #[inline]
    pub fn has_flag(&self, flag: u32) -> bool {
        self.flags.load(Ordering::Relaxed) & flag != 0
    }

    /// Set refcount
    #[inline]
    pub fn set_refcount(&self, count: u32) {
        self.refcount.store(count, Ordering::Relaxed);
    }

    /// Set owner PID
    #[inline]
    pub fn set_owner(&self, pid: u32) {
        self.owner.store(pid, Ordering::Relaxed);
    }

    /// Check if frame is free (flags == 0, refcount == 0)
    #[inline]
    pub fn is_free(&self) -> bool {
        self.flags.load(Ordering::Relaxed) == PF_FREE
            && self.refcount.load(Ordering::Relaxed) == 0
    }
}

// ============================================================================
// PageDbError — WireSaint: Each variant carries enough diagnostics to tell you
// exactly what went wrong, whose fault it is, and what the frame's state was
// at the moment of the crime. No more guessing.
// ============================================================================

/// Errors from PageDatabase operations
#[derive(Debug, Clone, Copy)]
pub enum PageDbError {
    /// Attempted to free a frame that's already free — double-free detected
    DoubleFree {
        phys: u64,
        current_flags: u32,
        owner: u32,
        context: u8,
    },
    /// Attempted to free a reserved frame — NEVER do this
    FreeReserved { phys: u64, context: u8 },
    /// Reference count underflowed below zero
    RefcountUnderflow { phys: u64, current: u32 },
    /// Physical address outside tracked range
    InvalidPfn { phys: u64 },
}

impl PageDbError {
    /// Dump error diagnostics to serial — BlackLatch: ISR-safe, lock-free,
    /// bounded output. When this fires, something is deeply wrong and we need
    /// every byte of context we can get.
    pub fn dump(&self) {
        unsafe {
            match self {
                PageDbError::DoubleFree {
                    phys,
                    current_flags,
                    owner,
                    context,
                } => {
                    os_log::write_str_raw("[PAGEDB-ERROR] DoubleFree caller=");
                    os_log::write_str_raw(context_name(*context));
                    os_log::write_str_raw(" phys=0x");
                    os_log::write_u64_hex_raw(*phys);
                    os_log::write_str_raw(" flags=0x");
                    os_log::write_u64_hex_raw(*current_flags as u64);
                    os_log::write_str_raw(" owner=PID(");
                    os_log::write_u32_raw(*owner);
                    os_log::write_str_raw(")\n");
                }
                PageDbError::FreeReserved { phys, context } => {
                    os_log::write_str_raw("[PAGEDB-ERROR] FreeReserved caller=");
                    os_log::write_str_raw(context_name(*context));
                    os_log::write_str_raw(" phys=0x");
                    os_log::write_u64_hex_raw(*phys);
                    os_log::write_str_raw("\n");
                }
                PageDbError::RefcountUnderflow { phys, current } => {
                    os_log::write_str_raw("[PAGEDB-ERROR] RefcountUnderflow phys=0x");
                    os_log::write_u64_hex_raw(*phys);
                    os_log::write_str_raw(" current=");
                    os_log::write_u32_raw(*current);
                    os_log::write_str_raw("\n");
                }
                PageDbError::InvalidPfn { phys } => {
                    os_log::write_str_raw("[PAGEDB-ERROR] InvalidPfn phys=0x");
                    os_log::write_u64_hex_raw(*phys);
                    os_log::write_str_raw("\n");
                }
            }
        }
    }
}

// ============================================================================
// PageDbStats — for /proc/meminfo and boot diagnostics
// ============================================================================

/// Summary statistics from the page frame database
#[derive(Debug, Clone, Copy, Default)]
pub struct PageDbStats {
    pub total: usize,
    pub free: usize,
    pub allocated: usize,
    pub mapped: usize,
    pub pagetable: usize,
    pub reserved: usize,
    pub cow_shared: usize,
    pub kernel: usize,
}

// ============================================================================
// PageDatabase — the global singleton. Flat array indexed by PFN.
// — GraveShift: This is the single source of truth for frame ownership.
// ============================================================================

/// Physical memory map base for direct access
const PHYS_MAP_BASE: u64 = 0xFFFF_8000_0000_0000;

/// The page frame database — flat array of PageFrame entries indexed by PFN
///
/// — SableWire: ALL fields are atomic. The old struct used plain *mut/usize/bool,
/// which meant init() required &mut self. But PAGE_DATABASE is a plain `static`
/// (no UnsafeCell), so mutating through &PAGE_DATABASE → *mut was instant UB.
/// In debug mode LLVM doesn't optimize, so it worked. In release mode LLVM saw
/// the "immutable" static, assumed count=0 forever, and optimized away the init.
/// Result: pagedb empty → buddy drops every block → compositor OOM → panic.
/// Atomics give us interior mutability without UnsafeCell. Crisis averted.
pub struct PageDatabase {
    /// Pointer to the flat array of PageFrame entries (in direct-map region)
    frames: AtomicPtr<PageFrame>,
    /// Total number of frames tracked
    count: AtomicUsize,
    /// Whether the database is initialized
    initialized: core::sync::atomic::AtomicBool,
}

/// — SableWire: SAFETY — PageDatabase uses raw pointers into the direct-map
/// region, which is globally accessible. The AtomicU32 fields provide
/// per-field synchronization. No mutable aliasing beyond atomic ops.
unsafe impl Send for PageDatabase {}
unsafe impl Sync for PageDatabase {}

// ============================================================================
// Global singleton — same pattern as mm-manager
// ============================================================================

static GLOBAL_PAGEDB: AtomicPtr<PageDatabase> = AtomicPtr::new(core::ptr::null_mut());

/// Initialize the global page database
///
/// # Safety
/// Must be called once during boot with a reference to a static PageDatabase.
pub unsafe fn init_global(db: &'static PageDatabase) {
    GLOBAL_PAGEDB.store(db as *const _ as *mut _, Ordering::Release);
}

/// Get a reference to the global page database
///
/// # Panics
/// Panics if the page database hasn't been initialized.
pub fn pagedb() -> &'static PageDatabase {
    let ptr = GLOBAL_PAGEDB.load(Ordering::Acquire);
    if ptr.is_null() {
        panic!("PageDatabase not initialized");
    }
    unsafe { &*ptr }
}

/// Try to get a reference to the global page database.
/// Returns None if not initialized — safe to call from buddy alloc/free
/// paths during early boot before the pagedb is set up.
pub fn try_pagedb() -> Option<&'static PageDatabase> {
    let ptr = GLOBAL_PAGEDB.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &*ptr })
    }
}

impl PageDatabase {
    /// Create a new uninitialized PageDatabase
    pub const fn new() -> Self {
        Self {
            frames: AtomicPtr::new(core::ptr::null_mut()),
            count: AtomicUsize::new(0),
            initialized: core::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Initialize the page database array
    ///
    /// # Safety
    /// `array_virt` must point to a zeroed region large enough for `frame_count` PageFrame entries.
    /// Must be called once during boot.
    /// — SableWire: now takes &self (not &mut self) thanks to atomic fields.
    /// No more UB from casting away immutability on a plain static.
    pub unsafe fn init(&self, array_virt: *mut PageFrame, frame_count: usize) {
        self.frames.store(array_virt, Ordering::Release);
        self.count.store(frame_count, Ordering::Release);
        self.initialized.store(true, Ordering::Release);
    }

    /// Check if initialized
    #[inline]
    pub fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::Acquire)
    }

    /// Total frame count
    #[inline]
    pub fn frame_count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }

    /// Convert physical address to PFN (page frame number)
    #[inline]
    fn phys_to_pfn(&self, phys: PhysAddr) -> Option<usize> {
        let pfn = (phys.as_u64() >> FRAME_SHIFT) as usize;
        if pfn < self.count.load(Ordering::Acquire) {
            Some(pfn)
        } else {
            None
        }
    }

    /// Get PageFrame for a physical address
    #[inline]
    pub fn get(&self, phys: PhysAddr) -> Option<&PageFrame> {
        let pfn = self.phys_to_pfn(phys)?;
        let frames = self.frames.load(Ordering::Acquire);
        if frames.is_null() {
            return None;
        }
        // SAFETY: pfn is bounds-checked by phys_to_pfn, frames is non-null
        Some(unsafe { &*frames.add(pfn) })
    }

    /// Mark frame as allocated (called from buddy alloc path)
    ///
    /// — TorqueJax: Sets ALLOCATED flag and refcount=1. Owner defaults to 0 (kernel)
    /// until the caller assigns it to a specific PID.
    pub fn mark_allocated(&self, phys: PhysAddr, owner: u32) {
        if let Some(frame) = self.get(phys) {
            let old_flags = frame.flags();
            let old_rc = frame.refcount();
            let ctx = FREE_CONTEXT.load(Ordering::Relaxed);
            record_event(phys.as_u64(), EventType::Alloc, ctx, old_flags, old_rc, frame.owner());
            frame.set_flags(PF_ALLOCATED);
            frame.set_refcount(1);
            frame.set_owner(owner);
        }
    }

    /// Mark frame as free (called from buddy free path after validation)
    ///
    /// — GraveShift: Zeroes everything. Frame goes back to the void.
    pub fn mark_free(&self, phys: PhysAddr) -> Result<(), PageDbError> {
        let pfn = self.phys_to_pfn(phys).ok_or(PageDbError::InvalidPfn {
            phys: phys.as_u64(),
        })?;
        let frames = self.frames.load(Ordering::Acquire);
        let frame = unsafe { &*frames.add(pfn) };
        let old_flags = frame.flags();
        let old_rc = frame.refcount();
        let ctx = FREE_CONTEXT.load(Ordering::Relaxed);
        record_event(phys.as_u64(), EventType::Free, ctx, old_flags, old_rc, frame.owner());
        frame.set_flags(PF_FREE);
        frame.set_refcount(0);
        frame.set_owner(0);
        frame.order_and_reserved.store(0, Ordering::Relaxed);
        Ok(())
    }

    /// Mark frame as page table structure (PML4/PDPT/PD/PT)
    pub fn mark_pagetable(&self, phys: PhysAddr, owner: u32) {
        if let Some(frame) = self.get(phys) {
            let old_flags = frame.flags();
            let old_rc = frame.refcount();
            record_event(phys.as_u64(), EventType::MarkPT, 0, old_flags, old_rc, frame.owner());
            frame.set_flags(PF_ALLOCATED | PF_PAGETABLE);
            frame.set_refcount(1);
            frame.set_owner(owner);
        }
    }

    /// Mark frame as reserved (boot PT frames, kernel code, MMIO, low memory)
    pub fn mark_reserved(&self, phys: PhysAddr) {
        if let Some(frame) = self.get(phys) {
            frame.set_flags(PF_RESERVED);
            frame.set_refcount(1);
            frame.set_owner(u32::MAX); // boot/reserved sentinel
        }
    }

    /// Mark frame as reserved + kernel-owned
    pub fn mark_reserved_kernel(&self, phys: PhysAddr) {
        if let Some(frame) = self.get(phys) {
            frame.set_flags(PF_RESERVED | PF_KERNEL);
            frame.set_refcount(1);
            frame.set_owner(u32::MAX);
        }
    }

    /// Mark frame as reserved page table (boot PT structure)
    pub fn mark_reserved_pagetable(&self, phys: PhysAddr) {
        if let Some(frame) = self.get(phys) {
            frame.set_flags(PF_RESERVED | PF_PAGETABLE);
            frame.set_refcount(1);
            frame.set_owner(u32::MAX);
        }
    }

    /// Increment refcount (COW share). Returns new refcount.
    pub fn ref_inc(&self, phys: PhysAddr) -> u32 {
        if let Some(frame) = self.get(phys) {
            let old_flags = frame.flags();
            let old_rc = frame.refcount();
            record_event(phys.as_u64(), EventType::RefInc, 0, old_flags, old_rc, frame.owner());
            frame.refcount.fetch_add(1, Ordering::Relaxed) + 1
        } else {
            0
        }
    }

    /// Decrement refcount (COW unshare / process exit). Returns new refcount.
    ///
    /// — WireSaint: Guard against decrementing freed frames. If the frame's
    /// flags are PF_FREE, it was already returned to buddy — the COW tracker
    /// has a stale entry. Skip the decrement to prevent cascading corruption
    /// (underflow → DoubleFree when Drop-pt-walk tries to free it).
    pub fn ref_dec(&self, phys: PhysAddr) -> u32 {
        if let Some(frame) = self.get(phys) {
            let flags = frame.flags();
            let old_rc = frame.refcount();
            record_event(phys.as_u64(), EventType::RefDec, FREE_CONTEXT.load(Ordering::Relaxed), flags, old_rc, frame.owner());

            // — WireSaint: Frame already freed. Don't touch refcount — the
            // BTreeMap has a stale entry for this frame. Log it but don't
            // underflow, which would cascade into DoubleFree downstream.
            if flags == PF_FREE && old_rc == 0 {
                unsafe {
                    os_log::write_str_raw("[PAGEDB-WARN] ref_dec on FREE frame phys=0x");
                    os_log::write_u64_hex_raw(phys.as_u64());
                    os_log::write_str_raw(" — skipped (stale COW entry)\n");
                }
                return 0;
            }

            let old = frame.refcount.fetch_sub(1, Ordering::Relaxed);
            if old == 0 {
                // — WireSaint: Underflow! Something decremented past zero.
                // Restore to 0 and report with full frame state.
                frame.refcount.store(0, Ordering::Relaxed);
                let err = PageDbError::RefcountUnderflow {
                    phys: phys.as_u64(),
                    current: 0,
                };
                err.dump();
                // Also dump full frame state for debugging
                self.dump_frame(phys);
                // — GraveShift: Dump the event ring to show what happened
                dump_event_ring_for(phys.as_u64());
                return 0;
            }
            old - 1
        } else {
            0
        }
    }

    /// Validate frame state before free — the corruption detector
    ///
    /// — ColdCipher: This is the money shot. Called BEFORE buddy free() adds
    /// the frame back to the free list. Catches double-frees, reserved frame
    /// frees, and any other insanity at the exact moment it happens.
    /// Reads the FREE_CONTEXT atomic for caller identification — set it
    /// with set_free_context() BEFORE calling free_frame.
    pub fn validate_free(&self, phys: PhysAddr) -> Result<(), PageDbError> {
        let pfn = self.phys_to_pfn(phys).ok_or(PageDbError::InvalidPfn {
            phys: phys.as_u64(),
        })?;
        let frames = self.frames.load(Ordering::Acquire);
        let frame = unsafe { &*frames.add(pfn) };
        let flags = frame.flags();
        let ctx = FREE_CONTEXT.load(Ordering::Relaxed);

        // Already free? Double-free!
        if flags == PF_FREE && frame.refcount() == 0 {
            let err = PageDbError::DoubleFree {
                phys: phys.as_u64(),
                current_flags: flags,
                owner: frame.owner(),
                context: ctx,
            };
            err.dump();
            // — GraveShift: Dump the event ring to show WHO freed it
            dump_event_ring_for(phys.as_u64());
            return Err(err);
        }

        // Reserved frames are NEVER freed
        if flags & PF_RESERVED != 0 {
            let err = PageDbError::FreeReserved {
                phys: phys.as_u64(),
                context: ctx,
            };
            err.dump();
            return Err(err);
        }

        Ok(())
    }

    /// Dump diagnostics for a single frame — serial output, ISR-safe
    pub fn dump_frame(&self, phys: PhysAddr) {
        if let Some(frame) = self.get(phys) {
            let flags = frame.flags();
            let rc = frame.refcount();
            let owner = frame.owner();

            unsafe {
                os_log::write_str_raw("[PAGEDB] frame=0x");
                os_log::write_u64_hex_raw(phys.as_u64());
                os_log::write_str_raw(" flags=0x");
                os_log::write_u64_hex_raw(flags as u64);
                os_log::write_str_raw("(");
                // Decode flags
                if flags == PF_FREE {
                    os_log::write_str_raw("FREE");
                } else {
                    let mut first = true;
                    if flags & PF_ALLOCATED != 0 {
                        os_log::write_str_raw("ALLOC");
                        first = false;
                    }
                    if flags & PF_MAPPED != 0 {
                        if !first { os_log::write_str_raw("|"); }
                        os_log::write_str_raw("MAPPED");
                        first = false;
                    }
                    if flags & PF_PAGETABLE != 0 {
                        if !first { os_log::write_str_raw("|"); }
                        os_log::write_str_raw("PT");
                        first = false;
                    }
                    if flags & PF_RESERVED != 0 {
                        if !first { os_log::write_str_raw("|"); }
                        os_log::write_str_raw("RSVD");
                        first = false;
                    }
                    if flags & PF_COW != 0 {
                        if !first { os_log::write_str_raw("|"); }
                        os_log::write_str_raw("COW");
                        first = false;
                    }
                    if flags & PF_KERNEL != 0 {
                        if !first { os_log::write_str_raw("|"); }
                        os_log::write_str_raw("KERN");
                        let _ = first;
                    }
                }
                os_log::write_str_raw(") rc=");
                os_log::write_u32_raw(rc);
                os_log::write_str_raw(" owner=PID(");
                os_log::write_u32_raw(owner);
                os_log::write_str_raw(")\n");
            }
        } else {
            unsafe {
                os_log::write_str_raw("[PAGEDB] frame=0x");
                os_log::write_u64_hex_raw(phys.as_u64());
                os_log::write_str_raw(" OUT_OF_RANGE\n");
            }
        }
    }

    /// Compute summary statistics by scanning the entire array
    ///
    /// — BlackLatch: O(N) scan over all frames. Only call during boot or
    /// from /proc/meminfo, never in a hot path.
    pub fn stats(&self) -> PageDbStats {
        let mut stats = PageDbStats::default();
        let count = self.count.load(Ordering::Acquire);
        let frames = self.frames.load(Ordering::Acquire);
        stats.total = count;

        if frames.is_null() {
            return stats;
        }

        for i in 0..count {
            let frame = unsafe { &*frames.add(i) };
            let flags = frame.flags();
            let rc = frame.refcount();

            if flags == PF_FREE && rc == 0 {
                stats.free += 1;
            } else if flags & PF_RESERVED != 0 {
                stats.reserved += 1;
            } else if flags & PF_PAGETABLE != 0 {
                stats.pagetable += 1;
            } else if flags & PF_MAPPED != 0 {
                stats.mapped += 1;
            } else if flags & PF_ALLOCATED != 0 {
                stats.allocated += 1;
            }

            if flags & PF_COW != 0 {
                stats.cow_shared += 1;
            }
            if flags & PF_KERNEL != 0 {
                stats.kernel += 1;
            }
        }

        stats
    }
}
