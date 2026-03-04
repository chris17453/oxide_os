//! TLB (Translation Lookaside Buffer) shootdown support
//!
//! When page tables are modified, other CPUs may have stale TLB entries.
//! TLB shootdown uses IPIs to coordinate TLB invalidation across CPUs.

use crate::IpiTarget;
use crate::cpu::cpus_online;
use crate::ipi::{send_ipi, vector};
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// TLB shootdown request stored in atomics for safe access
struct TlbShootdownState {
    /// Start address to invalidate
    start: AtomicU64,
    /// End address (exclusive)
    end: AtomicU64,
    /// Address space ID (0 = kernel)
    asid: AtomicU64,
    /// Number of CPUs that still need to acknowledge
    pending_acks: AtomicUsize,
    /// Lock for serializing shootdown requests
    in_progress: AtomicUsize,
}

impl TlbShootdownState {
    const fn new() -> Self {
        TlbShootdownState {
            start: AtomicU64::new(0),
            end: AtomicU64::new(0),
            asid: AtomicU64::new(0),
            pending_acks: AtomicUsize::new(0),
            in_progress: AtomicUsize::new(0),
        }
    }
}

static TLB_STATE: TlbShootdownState = TlbShootdownState::new();

/// Perform a TLB shootdown for a range of addresses
///
/// This invalidates TLB entries for the specified address range
/// on all CPUs. The function blocks until all CPUs have acknowledged.
pub fn tlb_shootdown(start: u64, end: u64, asid: u64) {
    let online = cpus_online() as usize;

    // If only one CPU, just invalidate locally
    if online <= 1 {
        invalidate_range(start, end);
        return;
    }

    // Acquire the shootdown lock (simple spinlock)
    while TLB_STATE
        .in_progress
        .compare_exchange(0, 1, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }

    // Set up the request
    TLB_STATE.start.store(start, Ordering::Release);
    TLB_STATE.end.store(end, Ordering::Release);
    TLB_STATE.asid.store(asid, Ordering::Release);

    // We need acks from all other CPUs (online - 1)
    TLB_STATE.pending_acks.store(online - 1, Ordering::Release);

    // Send IPI to all other CPUs
    send_ipi(IpiTarget::AllExceptSelf, vector::TLB_SHOOTDOWN);

    // Invalidate our own TLB
    invalidate_range(start, end);

    // Wait for all CPUs to acknowledge
    while TLB_STATE.pending_acks.load(Ordering::Acquire) > 0 {
        core::hint::spin_loop();
    }

    // Release the lock
    TLB_STATE.in_progress.store(0, Ordering::Release);
}

/// Handle TLB shootdown IPI
///
/// Called from the IPI handler on each CPU.
pub fn handle_tlb_shootdown() {
    let start = TLB_STATE.start.load(Ordering::Acquire);
    let end = TLB_STATE.end.load(Ordering::Acquire);

    // Invalidate the requested range
    invalidate_range(start, end);

    // Acknowledge completion
    TLB_STATE.pending_acks.fetch_sub(1, Ordering::Release);
}

/// Invalidate a single TLB entry
///
/// On x86_64, this uses the INVLPG instruction.
#[inline]
pub fn invalidate_page(addr: u64) {
    // Architecture-specific implementation
    // On x86_64: invlpg [addr]
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::asm!(
            "invlpg [{}]",
            in(reg) addr,
            options(nostack, preserves_flags)
        );
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // Other architectures would have their own implementation
        let _ = addr;
    }
}

/// Invalidate a range of TLB entries
pub fn invalidate_range(start: u64, end: u64) {
    const PAGE_SIZE: u64 = 4096;

    // Align to page boundaries — saturating to avoid overflow when end == u64::MAX
    // — SableWire: exec() passes (0, u64::MAX) for full address space flush.
    // Wrapping that around would ruin everybody's day.
    let start_aligned = start & !(PAGE_SIZE - 1);
    let end_aligned = end.saturating_add(PAGE_SIZE - 1) & !(PAGE_SIZE - 1);

    // If range is too large or wraps, just flush all
    if end_aligned <= start_aligned {
        flush_tlb_all();
        return;
    }
    let num_pages = (end_aligned - start_aligned) / PAGE_SIZE;
    if num_pages > 32 {
        // Full TLB flush is cheaper for large ranges
        flush_tlb_all();
        return;
    }

    // Invalidate each page — SableWire: wrapping_add because debug-mode overflow
    // panics at 0xFFFF_FFFF_FFFF_F000 + 0x1000 are not a good look
    let mut addr = start_aligned;
    while addr < end_aligned {
        invalidate_page(addr);
        addr = addr.wrapping_add(PAGE_SIZE);
    }
}

/// Flush the entire TLB
///
/// On x86_64, this reloads CR3.
pub fn flush_tlb_all() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // Reload CR3 to flush entire TLB
        let cr3: u64;
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nostack, preserves_flags));
        core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags));
    }

    #[cfg(not(target_arch = "x86_64"))]
    {
        // Other architectures would have their own implementation
    }
}

/// Flush TLB on all CPUs
pub fn flush_tlb_all_cpus() {
    tlb_shootdown(0, u64::MAX, 0);
}

/// Statistics for TLB shootdowns
pub struct TlbStats {
    /// Total shootdowns initiated
    pub shootdowns: AtomicU64,
    /// Total pages invalidated
    pub pages_invalidated: AtomicU64,
    /// Total full flushes
    pub full_flushes: AtomicU64,
}

impl TlbStats {
    pub const fn new() -> Self {
        TlbStats {
            shootdowns: AtomicU64::new(0),
            pages_invalidated: AtomicU64::new(0),
            full_flushes: AtomicU64::new(0),
        }
    }
}

static TLB_STATS: TlbStats = TlbStats::new();

/// Get TLB statistics
pub fn get_tlb_stats() -> &'static TlbStats {
    &TLB_STATS
}
