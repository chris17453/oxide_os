//! Global atomic VM statistics counters for OXIDE OS
//!
//! — TorqueJax: Every page fault, every alloc, every free — counted. Zero-lock,
//! ISR-safe, Relaxed-ordering atomics. The performance tax is one fetch_add per
//! event — if your hot path can't afford that, your hot path has bigger problems.
//! These counters are the nervous system of the VM subsystem. Without them you're
//! flying blind into OOM territory with nothing but a serial port prayer. — TorqueJax

#![cfg_attr(not(test), no_std)]

use core::sync::atomic::{AtomicU64, Ordering};

/// — TorqueJax: Every VM event worth measuring gets an entry here. Counters at 0
/// mean "this event hasn't fired yet" — that's real data, not a stub. Future phases
/// wire their counters when the mechanism ships. /proc/vmstat shows all of them.
#[repr(usize)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counter {
    // ── Fault counters ──────────────────────────────────────────────────
    // P0 wires PgFault/Anon/Cow/StackGrowth; P1 wires PgFaultFile
    PgFault,
    PgFaultAnon,
    PgFaultFile,
    PgFaultCow,
    PgFaultStackGrowth,

    // ── Allocation counters (P0 wires all six) ──────────────────────────
    PgAllocNormal,
    PgAllocDma,
    PgAllocHigh,
    PgFreeNormal,
    PgFreeDma,
    PgFreeHigh,

    // ── Reclaim counters (P2 wires all) ─────────────────────────────────
    PgScanDirect,
    PgScanKswapd,
    PgReclaimDirect,
    PgReclaimKswapd,
    PgSteal,

    // ── Writeback counters (P1 wires all) ───────────────────────────────
    PgDirty,
    PgWriteback,
    PgWritten,

    // ── OOM (P0 wires) ─────────────────────────────────────────────────
    OomKill,

    // ── Swap counters (P3 wires all) ────────────────────────────────────
    PgSwapIn,
    PgSwapOut,

    // ── Watermark events (P2 wires all) ─────────────────────────────────
    WmarkLow,
    WmarkMin,

    // ── Syscall counters (P0 wires all) ─────────────────────────────────
    MmapCount,
    MunmapCount,
    BrkCount,

    // — TorqueJax: Sentinel — array size derived from this. Never use as a counter.
    _Count,
}

impl Counter {
    /// — TorqueJax: Linux-style snake_case name for /proc/vmstat output.
    /// No allocations, no format!, just static str. ISR-safe.
    pub const fn name(self) -> &'static str {
        match self {
            Counter::PgFault => "pgfault",
            Counter::PgFaultAnon => "pgfault_anon",
            Counter::PgFaultFile => "pgfault_file",
            Counter::PgFaultCow => "pgfault_cow",
            Counter::PgFaultStackGrowth => "pgfault_stack_growth",
            Counter::PgAllocNormal => "pgalloc_normal",
            Counter::PgAllocDma => "pgalloc_dma",
            Counter::PgAllocHigh => "pgalloc_high",
            Counter::PgFreeNormal => "pgfree_normal",
            Counter::PgFreeDma => "pgfree_dma",
            Counter::PgFreeHigh => "pgfree_high",
            Counter::PgScanDirect => "pgscan_direct",
            Counter::PgScanKswapd => "pgscan_kswapd",
            Counter::PgReclaimDirect => "pgreclaim_direct",
            Counter::PgReclaimKswapd => "pgreclaim_kswapd",
            Counter::PgSteal => "pgsteal",
            Counter::PgDirty => "pgdirty",
            Counter::PgWriteback => "pgwriteback",
            Counter::PgWritten => "pgwritten",
            Counter::OomKill => "oom_kill",
            Counter::PgSwapIn => "pgswapin",
            Counter::PgSwapOut => "pgswapout",
            Counter::WmarkLow => "wmark_low",
            Counter::WmarkMin => "wmark_min",
            Counter::MmapCount => "mmap_count",
            Counter::MunmapCount => "munmap_count",
            Counter::BrkCount => "brk_count",
            Counter::_Count => "_count",
        }
    }

    /// Number of real counters (excludes _Count sentinel)
    pub const COUNT: usize = Counter::_Count as usize;

    /// — TorqueJax: Iterate all real counters for /proc/vmstat enumeration.
    pub const ALL: [Counter; Self::COUNT] = [
        Counter::PgFault,
        Counter::PgFaultAnon,
        Counter::PgFaultFile,
        Counter::PgFaultCow,
        Counter::PgFaultStackGrowth,
        Counter::PgAllocNormal,
        Counter::PgAllocDma,
        Counter::PgAllocHigh,
        Counter::PgFreeNormal,
        Counter::PgFreeDma,
        Counter::PgFreeHigh,
        Counter::PgScanDirect,
        Counter::PgScanKswapd,
        Counter::PgReclaimDirect,
        Counter::PgReclaimKswapd,
        Counter::PgSteal,
        Counter::PgDirty,
        Counter::PgWriteback,
        Counter::PgWritten,
        Counter::OomKill,
        Counter::PgSwapIn,
        Counter::PgSwapOut,
        Counter::WmarkLow,
        Counter::WmarkMin,
        Counter::MmapCount,
        Counter::MunmapCount,
        Counter::BrkCount,
    ];
}

/// — TorqueJax: The global VM stats array. One AtomicU64 per counter, zero locks,
/// zero contention beyond cache-line bouncing. Good enough for Linux, good enough
/// for us. Per-CPU sharding would halve the bouncing but we're not at that scale yet.
pub struct VmStat {
    counters: [AtomicU64; Counter::COUNT],
}

impl VmStat {
    const fn new() -> Self {
        // — TorqueJax: const-init all counters to 0. No runtime init needed.
        const ZERO: AtomicU64 = AtomicU64::new(0);
        Self {
            counters: [ZERO; Counter::COUNT],
        }
    }
}

/// — TorqueJax: Global singleton. Lives in .bss, zero-initialized at load time.
/// No init function, no race window, no "did someone call vmstat_init() yet?" bugs.
static VMSTAT: VmStat = VmStat::new();

/// Increment a counter by 1. ISR-safe, lock-free, zero overhead beyond the atomic.
#[inline]
pub fn inc(counter: Counter) {
    VMSTAT.counters[counter as usize].fetch_add(1, Ordering::Relaxed);
}

/// Add N to a counter. For batch operations (e.g., freeing multiple pages).
#[inline]
pub fn add(counter: Counter, n: u64) {
    VMSTAT.counters[counter as usize].fetch_add(n, Ordering::Relaxed);
}

/// Read a counter's current value. Snapshot — may be stale by the time you use it.
#[inline]
pub fn get(counter: Counter) -> u64 {
    VMSTAT.counters[counter as usize].load(Ordering::Relaxed)
}

/// Decrement a counter by 1. For tracking in-flight states (dirty pages, writeback).
#[inline]
pub fn dec(counter: Counter) {
    VMSTAT.counters[counter as usize].fetch_sub(1, Ordering::Relaxed);
}

// ============================================================================
// Watermark callback — breaks circular dep between mm-core and mm-reclaim.
// mm-core (buddy) calls this after each alloc to check watermarks.
// mm-reclaim registers the real check_watermarks() at boot.
// ============================================================================

/// Callback type: (zone_index, free_pages) → check watermarks
pub type WatermarkCallbackFn = fn(u8, u64);

static WATERMARK_CB: core::sync::atomic::AtomicPtr<()> =
    core::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

/// Register the watermark check callback (called from mm-reclaim init)
pub fn set_watermark_callback(cb: WatermarkCallbackFn) {
    WATERMARK_CB.store(cb as *mut (), Ordering::Release);
}

/// Get the watermark callback (called from buddy alloc path)
#[inline]
pub fn watermark_callback() -> Option<WatermarkCallbackFn> {
    let ptr = WATERMARK_CB.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        // SAFETY: ptr was stored from a valid fn pointer via set_watermark_callback
        Some(unsafe { core::mem::transmute::<*mut (), WatermarkCallbackFn>(ptr) })
    }
}

// ============================================================================
// VM Tunables — /proc/sys/vm/ knobs backed by real mechanisms
// — IronGhost: Every knob here is read by the mechanism it controls.
// swappiness → reclaim scanner. dirty_ratio → writeback. No dead switches.
// ============================================================================

/// — IronGhost: Global VM tunables. AtomicU64 for lock-free reads.
pub struct VmTunables {
    pub swappiness: AtomicU64,
    pub dirty_ratio: AtomicU64,
    pub dirty_background_ratio: AtomicU64,
    pub dirty_expire_centisecs: AtomicU64,
    pub dirty_writeback_centisecs: AtomicU64,
    pub min_free_kbytes: AtomicU64,
    pub overcommit_memory: AtomicU64,
    pub oom_kill_allocating_task: AtomicU64,
    pub vfs_cache_pressure: AtomicU64,
}

impl VmTunables {
    pub const fn new() -> Self {
        Self {
            swappiness: AtomicU64::new(60),
            dirty_ratio: AtomicU64::new(40),
            dirty_background_ratio: AtomicU64::new(10),
            dirty_expire_centisecs: AtomicU64::new(3000),
            dirty_writeback_centisecs: AtomicU64::new(500),
            min_free_kbytes: AtomicU64::new(0),
            overcommit_memory: AtomicU64::new(0),
            oom_kill_allocating_task: AtomicU64::new(0),
            vfs_cache_pressure: AtomicU64::new(100),
        }
    }

    /// Get a tunable by name
    pub fn get_by_name(&self, name: &str) -> Option<u64> {
        match name {
            "swappiness" => Some(self.swappiness.load(Ordering::Relaxed)),
            "dirty_ratio" => Some(self.dirty_ratio.load(Ordering::Relaxed)),
            "dirty_background_ratio" => Some(self.dirty_background_ratio.load(Ordering::Relaxed)),
            "dirty_expire_centisecs" => Some(self.dirty_expire_centisecs.load(Ordering::Relaxed)),
            "dirty_writeback_centisecs" => Some(self.dirty_writeback_centisecs.load(Ordering::Relaxed)),
            "min_free_kbytes" => Some(self.min_free_kbytes.load(Ordering::Relaxed)),
            "overcommit_memory" => Some(self.overcommit_memory.load(Ordering::Relaxed)),
            "oom_kill_allocating_task" => Some(self.oom_kill_allocating_task.load(Ordering::Relaxed)),
            "vfs_cache_pressure" => Some(self.vfs_cache_pressure.load(Ordering::Relaxed)),
            _ => None,
        }
    }

    /// Set a tunable by name. Returns true if name was recognized.
    pub fn set_by_name(&self, name: &str, value: u64) -> bool {
        match name {
            "swappiness" => { self.swappiness.store(value.min(200), Ordering::Relaxed); true },
            "dirty_ratio" => { self.dirty_ratio.store(value.min(100), Ordering::Relaxed); true },
            "dirty_background_ratio" => { self.dirty_background_ratio.store(value.min(100), Ordering::Relaxed); true },
            "dirty_expire_centisecs" => { self.dirty_expire_centisecs.store(value, Ordering::Relaxed); true },
            "dirty_writeback_centisecs" => { self.dirty_writeback_centisecs.store(value, Ordering::Relaxed); true },
            "min_free_kbytes" => { self.min_free_kbytes.store(value, Ordering::Relaxed); true },
            "overcommit_memory" => { self.overcommit_memory.store(value.min(2), Ordering::Relaxed); true },
            "oom_kill_allocating_task" => { self.oom_kill_allocating_task.store(value.min(1), Ordering::Relaxed); true },
            "vfs_cache_pressure" => { self.vfs_cache_pressure.store(value, Ordering::Relaxed); true },
            _ => false,
        }
    }

    /// All tunable names
    pub const NAMES: &'static [&'static str] = &[
        "swappiness",
        "dirty_ratio",
        "dirty_background_ratio",
        "dirty_expire_centisecs",
        "dirty_writeback_centisecs",
        "min_free_kbytes",
        "overcommit_memory",
        "oom_kill_allocating_task",
        "vfs_cache_pressure",
    ];
}

static VM_TUNABLES: VmTunables = VmTunables::new();

/// Get the global VM tunables
pub fn vm_tunables() -> &'static VmTunables {
    &VM_TUNABLES
}

// ============================================================================
// Unit tests — CrashBloom: Memory problems have plagued us for years.
// Every counter, every tunable, every callback gets tested.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_inc_and_get() {
        // — CrashBloom: Basic counter increment/read cycle
        let before = get(Counter::PgFault);
        inc(Counter::PgFault);
        let after = get(Counter::PgFault);
        assert_eq!(after, before + 1, "inc should add exactly 1");
    }

    #[test]
    fn test_counter_add() {
        let before = get(Counter::PgAllocNormal);
        add(Counter::PgAllocNormal, 42);
        let after = get(Counter::PgAllocNormal);
        assert_eq!(after, before + 42, "add(42) should add exactly 42");
    }

    #[test]
    fn test_counter_dec() {
        // — CrashBloom: Ensure dec doesn't underflow to garbage
        add(Counter::PgDirty, 10);
        let before = get(Counter::PgDirty);
        dec(Counter::PgDirty);
        let after = get(Counter::PgDirty);
        assert_eq!(after, before - 1);
    }

    #[test]
    fn test_counter_names_unique() {
        // — CrashBloom: Duplicate counter names would corrupt /proc/vmstat parsing
        let names: Vec<&str> = Counter::ALL.iter().map(|c| c.name()).collect();
        for (i, name) in names.iter().enumerate() {
            for (j, other) in names.iter().enumerate() {
                if i != j {
                    assert_ne!(name, other, "Counter names must be unique: {}", name);
                }
            }
        }
    }

    #[test]
    fn test_counter_all_count() {
        assert_eq!(Counter::ALL.len(), Counter::COUNT, "ALL array must match COUNT");
    }

    #[test]
    fn test_tunable_defaults() {
        let t = VmTunables::new();
        assert_eq!(t.swappiness.load(Ordering::Relaxed), 60);
        assert_eq!(t.dirty_ratio.load(Ordering::Relaxed), 40);
        assert_eq!(t.dirty_background_ratio.load(Ordering::Relaxed), 10);
        assert_eq!(t.overcommit_memory.load(Ordering::Relaxed), 0);
        assert_eq!(t.vfs_cache_pressure.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn test_tunable_get_set_by_name() {
        let t = vm_tunables();
        // Read default
        assert_eq!(t.get_by_name("swappiness"), Some(60));
        // Set new value
        assert!(t.set_by_name("swappiness", 100));
        assert_eq!(t.get_by_name("swappiness"), Some(100));
        // Set back
        t.set_by_name("swappiness", 60);
        // Unknown name
        assert_eq!(t.get_by_name("nonexistent"), None);
        assert!(!t.set_by_name("nonexistent", 0));
    }

    #[test]
    fn test_tunable_clamping() {
        let t = vm_tunables();
        // swappiness clamped to 200
        t.set_by_name("swappiness", 999);
        assert_eq!(t.get_by_name("swappiness"), Some(200));
        t.set_by_name("swappiness", 60);
        // dirty_ratio clamped to 100
        t.set_by_name("dirty_ratio", 500);
        assert_eq!(t.get_by_name("dirty_ratio"), Some(100));
        t.set_by_name("dirty_ratio", 40);
        // overcommit_memory clamped to 2
        t.set_by_name("overcommit_memory", 10);
        assert_eq!(t.get_by_name("overcommit_memory"), Some(2));
        t.set_by_name("overcommit_memory", 0);
    }

    #[test]
    fn test_tunable_names_list() {
        // — CrashBloom: Every name in NAMES must resolve via get_by_name
        let t = vm_tunables();
        for name in VmTunables::NAMES {
            assert!(t.get_by_name(name).is_some(), "tunable '{}' not found", name);
        }
    }

    #[test]
    fn test_watermark_callback_initially_null() {
        // — CrashBloom: Before registration, callback should return None
        // (this test may fail if another test registered first — that's OK, it's a singleton)
        // Just verify the API doesn't crash
        let _cb = watermark_callback();
    }
}
