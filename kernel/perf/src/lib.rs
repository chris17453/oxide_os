//! Performance monitoring infrastructure for OXIDE OS
//!
//! Inspired by Linux perf_events, provides comprehensive performance counters
//! for interrupt handlers, locks, scheduler, and system calls.
//!
//! — PatchBay: "If you can't measure it, you can't optimize it."

#![no_std]

pub mod output;
pub mod stats;

use core::sync::atomic::{AtomicU64, AtomicU32, Ordering};

/// Performance event counters (Linux perf_event style)
///
/// These counters are updated atomically from interrupt context and can be
/// read safely from any context.
pub struct PerfCounters {
    // Timer interrupt statistics
    pub timer_irq_count: AtomicU64,
    pub timer_irq_cycles_total: AtomicU64,
    pub timer_irq_cycles_min: AtomicU64,
    pub timer_irq_cycles_max: AtomicU64,

    // Keyboard interrupt statistics
    pub keyboard_irq_count: AtomicU64,
    pub keyboard_irq_cycles_total: AtomicU64,

    // Mouse interrupt statistics
    pub mouse_irq_count: AtomicU64,
    pub mouse_irq_cycles_total: AtomicU64,

    // Scheduler statistics
    pub context_switches: AtomicU64,
    pub preemptions: AtomicU64,
    pub need_resched_set: AtomicU64,

    // Lock contention counters
    pub terminal_lock_contentions: AtomicU64,
    pub scheduler_lock_contentions: AtomicU64,
    pub vt_lock_contentions: AtomicU64,

    // System call statistics
    pub syscall_count: AtomicU64,
    pub syscall_cycles_total: AtomicU64,
    pub syscall_slow_count: AtomicU32, // Syscalls > 100K cycles

    // Serial output statistics
    pub serial_bytes_written: AtomicU64,
    pub serial_bytes_dropped: AtomicU64,
    pub serial_spin_limit_hits: AtomicU64,

    // Terminal rendering statistics
    pub terminal_ticks: AtomicU64,
    pub terminal_renders: AtomicU64,
    pub terminal_render_cycles: AtomicU64,

    // Mouse processing statistics
    pub mouse_events_processed: AtomicU64,
    pub mouse_events_dropped: AtomicU64,
}

impl PerfCounters {
    pub const fn new() -> Self {
        Self {
            timer_irq_count: AtomicU64::new(0),
            timer_irq_cycles_total: AtomicU64::new(0),
            timer_irq_cycles_min: AtomicU64::new(u64::MAX),
            timer_irq_cycles_max: AtomicU64::new(0),

            keyboard_irq_count: AtomicU64::new(0),
            keyboard_irq_cycles_total: AtomicU64::new(0),

            mouse_irq_count: AtomicU64::new(0),
            mouse_irq_cycles_total: AtomicU64::new(0),

            context_switches: AtomicU64::new(0),
            preemptions: AtomicU64::new(0),
            need_resched_set: AtomicU64::new(0),

            terminal_lock_contentions: AtomicU64::new(0),
            scheduler_lock_contentions: AtomicU64::new(0),
            vt_lock_contentions: AtomicU64::new(0),

            syscall_count: AtomicU64::new(0),
            syscall_cycles_total: AtomicU64::new(0),
            syscall_slow_count: AtomicU32::new(0),

            serial_bytes_written: AtomicU64::new(0),
            serial_bytes_dropped: AtomicU64::new(0),
            serial_spin_limit_hits: AtomicU64::new(0),

            terminal_ticks: AtomicU64::new(0),
            terminal_renders: AtomicU64::new(0),
            terminal_render_cycles: AtomicU64::new(0),

            mouse_events_processed: AtomicU64::new(0),
            mouse_events_dropped: AtomicU64::new(0),
        }
    }

    /// Record timer interrupt execution (call at ISR entry and exit)
    #[inline]
    pub fn record_timer_irq(&self, cycles: u64) {
        self.timer_irq_count.fetch_add(1, Ordering::Relaxed);
        self.timer_irq_cycles_total.fetch_add(cycles, Ordering::Relaxed);

        // Update min (fetch_min not in stable Rust, manual CAS loop)
        let mut current_min = self.timer_irq_cycles_min.load(Ordering::Relaxed);
        while cycles < current_min {
            match self.timer_irq_cycles_min.compare_exchange_weak(
                current_min,
                cycles,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_min = actual,
            }
        }

        // Update max
        let mut current_max = self.timer_irq_cycles_max.load(Ordering::Relaxed);
        while cycles > current_max {
            match self.timer_irq_cycles_max.compare_exchange_weak(
                current_max,
                cycles,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(actual) => current_max = actual,
            }
        }
    }

    /// Record keyboard interrupt
    #[inline]
    pub fn record_keyboard_irq(&self, cycles: u64) {
        self.keyboard_irq_count.fetch_add(1, Ordering::Relaxed);
        self.keyboard_irq_cycles_total.fetch_add(cycles, Ordering::Relaxed);
    }

    /// Record mouse interrupt
    #[inline]
    pub fn record_mouse_irq(&self, cycles: u64) {
        self.mouse_irq_count.fetch_add(1, Ordering::Relaxed);
        self.mouse_irq_cycles_total.fetch_add(cycles, Ordering::Relaxed);
    }

    /// Record context switch
    #[inline]
    pub fn record_context_switch(&self) {
        self.context_switches.fetch_add(1, Ordering::Relaxed);
    }

    /// Record preemption
    #[inline]
    pub fn record_preemption(&self) {
        self.preemptions.fetch_add(1, Ordering::Relaxed);
    }

    /// Record need_resched being set
    #[inline]
    pub fn record_need_resched(&self) {
        self.need_resched_set.fetch_add(1, Ordering::Relaxed);
    }

    /// Record terminal lock contention
    #[inline]
    pub fn record_terminal_lock_contention(&self) {
        self.terminal_lock_contentions.fetch_add(1, Ordering::Relaxed);
    }

    /// Record scheduler lock contention
    #[inline]
    pub fn record_scheduler_lock_contention(&self) {
        self.scheduler_lock_contentions.fetch_add(1, Ordering::Relaxed);
    }

    /// Record VT lock contention
    #[inline]
    pub fn record_vt_lock_contention(&self) {
        self.vt_lock_contentions.fetch_add(1, Ordering::Relaxed);
    }

    /// Record syscall execution
    #[inline]
    pub fn record_syscall(&self, cycles: u64) {
        self.syscall_count.fetch_add(1, Ordering::Relaxed);
        self.syscall_cycles_total.fetch_add(cycles, Ordering::Relaxed);

        // Flag slow syscalls (> 100K cycles ~= 33us @ 3GHz)
        if cycles > 100_000 {
            self.syscall_slow_count.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record serial byte written
    #[inline]
    pub fn record_serial_write(&self, dropped: bool) {
        if dropped {
            self.serial_bytes_dropped.fetch_add(1, Ordering::Relaxed);
        } else {
            self.serial_bytes_written.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record serial spin limit hit
    #[inline]
    pub fn record_serial_spin_limit(&self) {
        self.serial_spin_limit_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record terminal tick
    #[inline]
    pub fn record_terminal_tick(&self) {
        self.terminal_ticks.fetch_add(1, Ordering::Relaxed);
    }

    /// Record terminal render
    #[inline]
    pub fn record_terminal_render(&self, cycles: u64) {
        self.terminal_renders.fetch_add(1, Ordering::Relaxed);
        self.terminal_render_cycles.fetch_add(cycles, Ordering::Relaxed);
    }

    /// Record mouse event processing
    #[inline]
    pub fn record_mouse_event(&self, dropped: bool) {
        if dropped {
            self.mouse_events_dropped.fetch_add(1, Ordering::Relaxed);
        } else {
            self.mouse_events_processed.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Get timer IRQ average cycles
    pub fn timer_irq_avg_cycles(&self) -> u64 {
        let count = self.timer_irq_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0;
        }
        let total = self.timer_irq_cycles_total.load(Ordering::Relaxed);
        total / count
    }

    /// Get keyboard IRQ average cycles
    pub fn keyboard_irq_avg_cycles(&self) -> u64 {
        let count = self.keyboard_irq_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0;
        }
        let total = self.keyboard_irq_cycles_total.load(Ordering::Relaxed);
        total / count
    }

    /// Get mouse IRQ average cycles
    pub fn mouse_irq_avg_cycles(&self) -> u64 {
        let count = self.mouse_irq_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0;
        }
        let total = self.mouse_irq_cycles_total.load(Ordering::Relaxed);
        total / count
    }

    /// Get syscall average cycles
    pub fn syscall_avg_cycles(&self) -> u64 {
        let count = self.syscall_count.load(Ordering::Relaxed);
        if count == 0 {
            return 0;
        }
        let total = self.syscall_cycles_total.load(Ordering::Relaxed);
        total / count
    }

    /// Get terminal render average cycles
    pub fn terminal_render_avg_cycles(&self) -> u64 {
        let count = self.terminal_renders.load(Ordering::Relaxed);
        if count == 0 {
            return 0;
        }
        let total = self.terminal_render_cycles.load(Ordering::Relaxed);
        total / count
    }

    /// Get serial drop rate (percentage)
    pub fn serial_drop_rate(&self) -> u64 {
        let written = self.serial_bytes_written.load(Ordering::Relaxed);
        let dropped = self.serial_bytes_dropped.load(Ordering::Relaxed);
        let total = written + dropped;
        if total == 0 {
            return 0;
        }
        (dropped * 100) / total
    }
}

/// Global performance counters (similar to Linux's per-cpu perf_event_context)
static PERF: PerfCounters = PerfCounters::new();

/// Get reference to global performance counters
#[inline]
pub fn counters() -> &'static PerfCounters {
    &PERF
}

/// Helper: Read CPU timestamp counter (x86_64 RDTSC)
///
/// — PatchBay: TSC is the gold standard for cycle-accurate profiling.
/// On modern CPUs (invariant TSC), it's constant-rate and synchronized across cores.
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn rdtsc() -> u64 {
    unsafe {
        let low: u32;
        let high: u32;
        core::arch::asm!(
            "rdtsc",
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
        ((high as u64) << 32) | (low as u64)
    }
}

/// Helper: Serialize instruction execution (x86_64 LFENCE)
///
/// Use before RDTSC to ensure all prior instructions have completed.
/// LFENCE is the standard way to serialize on modern CPUs.
#[cfg(target_arch = "x86_64")]
#[inline]
pub fn serialize() {
    unsafe {
        core::arch::asm!(
            "lfence",
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// Performance event scope guard (RAII for automatic measurement)
///
/// Usage:
/// ```
/// let _guard = PerfScope::timer_irq();
/// // ... ISR code ...
/// // Cycles automatically recorded on drop
/// ```
pub struct PerfScope {
    start_cycles: u64,
    event_type: EventType,
}

#[derive(Clone, Copy)]
enum EventType {
    TimerIrq,
    KeyboardIrq,
    MouseIrq,
    Syscall,
    TerminalRender,
}

impl PerfScope {
    /// Create timer IRQ perf scope
    #[inline]
    pub fn timer_irq() -> Self {
        Self {
            start_cycles: rdtsc(),
            event_type: EventType::TimerIrq,
        }
    }

    /// Create keyboard IRQ perf scope
    #[inline]
    pub fn keyboard_irq() -> Self {
        Self {
            start_cycles: rdtsc(),
            event_type: EventType::KeyboardIrq,
        }
    }

    /// Create mouse IRQ perf scope
    #[inline]
    pub fn mouse_irq() -> Self {
        Self {
            start_cycles: rdtsc(),
            event_type: EventType::MouseIrq,
        }
    }

    /// Create syscall perf scope
    #[inline]
    pub fn syscall() -> Self {
        Self {
            start_cycles: rdtsc(),
            event_type: EventType::Syscall,
        }
    }

    /// Create terminal render perf scope
    #[inline]
    pub fn terminal_render() -> Self {
        Self {
            start_cycles: rdtsc(),
            event_type: EventType::TerminalRender,
        }
    }
}

impl Drop for PerfScope {
    #[inline]
    fn drop(&mut self) {
        let end_cycles = rdtsc();
        let elapsed = end_cycles.saturating_sub(self.start_cycles);

        match self.event_type {
            EventType::TimerIrq => PERF.record_timer_irq(elapsed),
            EventType::KeyboardIrq => PERF.record_keyboard_irq(elapsed),
            EventType::MouseIrq => PERF.record_mouse_irq(elapsed),
            EventType::Syscall => PERF.record_syscall(elapsed),
            EventType::TerminalRender => PERF.record_terminal_render(elapsed),
        }
    }
}
