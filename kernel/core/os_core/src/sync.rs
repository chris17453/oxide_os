//! Synchronization primitives
//!
//! — GraveShift: Two flavors of mutex for two different worlds.
//! spin::Mutex — raw spinlock. No preemption awareness. Fine for ISR-reachable
//! code where we already know the scheduler won't touch us.
//! KernelMutex — Linux-model spinlock. Disables preemption on lock, re-enables
//! on unlock. Prevents the classic "preempted while holding lock → next task
//! deadlocks on the same lock" pattern. Use this for anything the scheduler's
//! timer ISR might interrupt (heap, VFS, block I/O paths).

pub use spin::{Mutex, MutexGuard};

use core::sync::atomic::{AtomicUsize, Ordering};
use core::ops::{Deref, DerefMut};

// ============================================================================
// Preemption hook registration
//
// — GraveShift: os_core can't depend on arch-x86_64 (circular dep). So we use
// function pointer callbacks registered at boot. Before registration, the hooks
// are no-ops — safe for early boot before the arch crate is initialized.
// After init.rs calls register_preempt_hooks(), every KernelMutex lock/unlock
// routes through arch::preempt_disable/enable.
// ============================================================================

static PREEMPT_DISABLE_FN: AtomicUsize = AtomicUsize::new(0);
static PREEMPT_ENABLE_FN: AtomicUsize = AtomicUsize::new(0);

/// Register preemption control hooks. Called once from kernel init.
/// — GraveShift: After this call, KernelMutex actually disables preemption.
/// Before this call, it's just a regular spinlock. Which is fine — during early
/// boot there's no scheduler to preempt us anyway.
pub fn register_preempt_hooks(disable: fn(), enable: fn()) {
    PREEMPT_DISABLE_FN.store(disable as usize, Ordering::Release);
    PREEMPT_ENABLE_FN.store(enable as usize, Ordering::Release);
}

#[inline]
fn call_preempt_disable() {
    let f = PREEMPT_DISABLE_FN.load(Ordering::Acquire);
    if f != 0 {
        // SAFETY: f was stored from a valid fn() pointer in register_preempt_hooks
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

#[inline]
fn call_preempt_enable() {
    let f = PREEMPT_ENABLE_FN.load(Ordering::Acquire);
    if f != 0 {
        // SAFETY: f was stored from a valid fn() pointer in register_preempt_hooks
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

// ============================================================================
// Legacy preemption API — allow/disallow kernel preemption
//
// — GraveShift: The old kpo (kernel_preempt_ok) API is used by ~60 call sites
// in blocking syscalls (nanosleep, poll, select, connect, flock). These set
// preemption to "allowed" before HLT and back to "disallowed" after wakeup.
// Routed through function pointers so os_core stays arch-independent.
// Registered alongside the preempt_disable/enable hooks at boot.
// ============================================================================

static ALLOW_PREEMPT_FN: AtomicUsize = AtomicUsize::new(0);
static DISALLOW_PREEMPT_FN: AtomicUsize = AtomicUsize::new(0);

/// Register legacy preemption control hooks. Called once from kernel init.
///
/// — GraveShift: These are separate from the KernelMutex preempt_disable/enable
/// hooks. allow_kernel_preempt sets preempt_count to 0 (fully preemptable),
/// disallow_kernel_preempt sets it to 1. Used by blocking syscall HLT loops.
pub fn register_preempt_control(allow: fn(), disallow: fn()) {
    ALLOW_PREEMPT_FN.store(allow as usize, Ordering::Release);
    DISALLOW_PREEMPT_FN.store(disallow as usize, Ordering::Release);
}

/// Allow kernel preemption (for blocking syscall HLT loops).
///
/// — GraveShift: Call before `sti; hlt` in yielding syscalls. Sets preempt
/// counter to 0 so the timer ISR can context-switch away from us. No-op
/// before registration (safe during early boot — no scheduler to preempt us).
#[inline]
pub fn allow_kernel_preempt() {
    let f = ALLOW_PREEMPT_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

/// Disallow kernel preemption (after wakeup from HLT).
///
/// — GraveShift: Call after returning from HLT in yielding syscalls. Restores
/// preempt counter to 1 so we don't get yanked mid-critical-section.
#[inline]
pub fn disallow_kernel_preempt() {
    let f = DISALLOW_PREEMPT_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

// ============================================================================
// Arch-independent CPU operations
//
// — NeonRoot: subsystem crates (syscall, sched, terminal, etc.) can't use
// `crate::arch` because they're separate crates. These function-pointer hooks
// let them call arch operations without depending on any arch crate.
// Registered once at boot from kernel init.
// ============================================================================

static USER_ACCESS_BEGIN_FN: AtomicUsize = AtomicUsize::new(0);
static USER_ACCESS_END_FN: AtomicUsize = AtomicUsize::new(0);
static WAIT_FOR_INTERRUPT_FN: AtomicUsize = AtomicUsize::new(0);
static ENABLE_INTERRUPTS_FN: AtomicUsize = AtomicUsize::new(0);
static DISABLE_INTERRUPTS_FN: AtomicUsize = AtomicUsize::new(0);

/// Register arch CPU operation hooks. Called once from kernel init.
///
/// — NeonRoot: After this, os_core::user_access_begin() etc. route to
/// the arch crate's trait implementations. Before this call they're no-ops.
pub fn register_arch_ops(
    user_begin: unsafe fn(),
    user_end: unsafe fn(),
    wait_int: fn(),
    enable_int: fn(),
    disable_int: fn(),
) {
    USER_ACCESS_BEGIN_FN.store(user_begin as usize, Ordering::Release);
    USER_ACCESS_END_FN.store(user_end as usize, Ordering::Release);
    WAIT_FOR_INTERRUPT_FN.store(wait_int as usize, Ordering::Release);
    ENABLE_INTERRUPTS_FN.store(enable_int as usize, Ordering::Release);
    DISABLE_INTERRUPTS_FN.store(disable_int as usize, Ordering::Release);
}

/// Begin user-memory access (SMAP/PAN disable). x86 = STAC, ARM = clear PAN.
///
/// # Safety
/// Must pair with `user_access_end()`.
#[inline]
pub unsafe fn user_access_begin() {
    let f = USER_ACCESS_BEGIN_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn() = unsafe { core::mem::transmute(f) };
        unsafe { func(); }
    }
}

/// End user-memory access (SMAP/PAN re-enable). x86 = CLAC, ARM = set PAN.
#[inline]
pub unsafe fn user_access_end() {
    let f = USER_ACCESS_END_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn() = unsafe { core::mem::transmute(f) };
        unsafe { func(); }
    }
}

/// Enable interrupts and wait for the next interrupt (idle/sleep).
/// x86 = sti+hlt (atomic), ARM = wfi.
#[inline]
pub fn wait_for_interrupt() {
    let f = WAIT_FOR_INTERRUPT_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

/// Enable interrupts. x86 = sti, ARM = daifclr.
#[inline]
pub fn enable_interrupts() {
    let f = ENABLE_INTERRUPTS_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

/// Disable interrupts. x86 = cli, ARM = daifset.
#[inline]
pub fn disable_interrupts() {
    let f = DISABLE_INTERRUPTS_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

// ============================================================================
// Port I/O operations
//
// — NeonRoot: subsystem crates need port I/O for serial debug, device access,
// etc. These dispatch through function pointers registered at boot.
// Drivers that are inherently arch-specific use cfg-gated type aliases instead.
// On non-x86 arches these stay unregistered and return safe defaults (0xFF/noop).
// ============================================================================

static PORT_INB_FN: AtomicUsize = AtomicUsize::new(0);
static PORT_OUTB_FN: AtomicUsize = AtomicUsize::new(0);
static PORT_INW_FN: AtomicUsize = AtomicUsize::new(0);
static PORT_OUTW_FN: AtomicUsize = AtomicUsize::new(0);
static PORT_INL_FN: AtomicUsize = AtomicUsize::new(0);
static PORT_OUTL_FN: AtomicUsize = AtomicUsize::new(0);

/// Register port I/O hooks. Called once from kernel init.
/// — NeonRoot: after this, os_core::inb/outb/etc. route to real arch port ops.
pub fn register_port_io(
    inb: unsafe fn(u16) -> u8,
    outb: unsafe fn(u16, u8),
    inw: unsafe fn(u16) -> u16,
    outw: unsafe fn(u16, u16),
    inl: unsafe fn(u16) -> u32,
    outl: unsafe fn(u16, u32),
) {
    PORT_INB_FN.store(inb as usize, Ordering::Release);
    PORT_OUTB_FN.store(outb as usize, Ordering::Release);
    PORT_INW_FN.store(inw as usize, Ordering::Release);
    PORT_OUTW_FN.store(outw as usize, Ordering::Release);
    PORT_INL_FN.store(inl as usize, Ordering::Release);
    PORT_OUTL_FN.store(outl as usize, Ordering::Release);
}

/// Read a byte from I/O port. Returns 0xFF before registration (bus float).
/// — NeonRoot: safe default mirrors what an unpopulated bus returns.
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let f = PORT_INB_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn(u16) -> u8 = unsafe { core::mem::transmute(f) };
        unsafe { func(port) }
    } else {
        0xFF
    }
}

/// Write a byte to I/O port. No-op before registration.
#[inline]
pub unsafe fn outb(port: u16, value: u8) {
    let f = PORT_OUTB_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn(u16, u8) = unsafe { core::mem::transmute(f) };
        unsafe { func(port, value); }
    }
}

/// Read a word from I/O port. Returns 0xFFFF before registration.
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let f = PORT_INW_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn(u16) -> u16 = unsafe { core::mem::transmute(f) };
        unsafe { func(port) }
    } else {
        0xFFFF
    }
}

/// Write a word to I/O port. No-op before registration.
#[inline]
pub unsafe fn outw(port: u16, value: u16) {
    let f = PORT_OUTW_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn(u16, u16) = unsafe { core::mem::transmute(f) };
        unsafe { func(port, value); }
    }
}

/// Read a dword from I/O port. Returns 0xFFFFFFFF before registration.
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let f = PORT_INL_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn(u16) -> u32 = unsafe { core::mem::transmute(f) };
        unsafe { func(port) }
    } else {
        0xFFFFFFFF
    }
}

/// Write a dword to I/O port. No-op before registration.
#[inline]
pub unsafe fn outl(port: u16, value: u32) {
    let f = PORT_OUTL_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn(u16, u32) = unsafe { core::mem::transmute(f) };
        unsafe { func(port, value); }
    }
}

// ============================================================================
// System registers, TSC, CPUID, memory fences
//
// — GraveShift: subsystem crates need MSR access (scheduler FS/GS base),
// TSC reads (perf, syscall timing), CPUID (procfs), and memory fences (perf).
// Same pattern — function pointers registered at boot. Before registration
// everything returns safe zeros / no-ops so early-boot code doesn't explode.
// ============================================================================

static READ_MSR_FN: AtomicUsize = AtomicUsize::new(0);
static WRITE_MSR_FN: AtomicUsize = AtomicUsize::new(0);
static READ_TSC_FN: AtomicUsize = AtomicUsize::new(0);
static CPUID_FN: AtomicUsize = AtomicUsize::new(0);
static MEMORY_FENCE_FN: AtomicUsize = AtomicUsize::new(0);
static READ_FENCE_FN: AtomicUsize = AtomicUsize::new(0);

/// Register system operations hooks. Called once from kernel init.
/// — GraveShift: wires up MSR/TSC/CPUID/fence dispatch. Until this runs,
/// every call returns zero or is a no-op. That's fine — the scheduler isn't
/// up yet and nobody is measuring time.
pub fn register_sys_ops(
    read_msr: unsafe fn(u32) -> u64,
    write_msr: unsafe fn(u32, u64),
    read_tsc: fn() -> u64,
    cpuid: fn(u32, u32) -> (u32, u32, u32, u32),
    mem_fence: fn(),
    read_fence: fn(),
) {
    READ_MSR_FN.store(read_msr as usize, Ordering::Release);
    WRITE_MSR_FN.store(write_msr as usize, Ordering::Release);
    READ_TSC_FN.store(read_tsc as usize, Ordering::Release);
    CPUID_FN.store(cpuid as usize, Ordering::Release);
    MEMORY_FENCE_FN.store(mem_fence as usize, Ordering::Release);
    READ_FENCE_FN.store(read_fence as usize, Ordering::Release);
}

/// Read a model-specific register. Returns 0 before registration.
/// — GraveShift: x86 rdmsr. ARM/MIPS route to their equivalent sys-reg access.
#[inline]
pub unsafe fn read_msr(id: u32) -> u64 {
    let f = READ_MSR_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn(u32) -> u64 = unsafe { core::mem::transmute(f) };
        unsafe { func(id) }
    } else {
        0
    }
}

/// Write a model-specific register. No-op before registration.
/// — GraveShift: x86 wrmsr. Touching the wrong MSR is a one-way ticket to #GP.
#[inline]
pub unsafe fn write_msr(id: u32, value: u64) {
    let f = WRITE_MSR_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn(u32, u64) = unsafe { core::mem::transmute(f) };
        unsafe { func(id, value); }
    }
}

/// Read timestamp counter. Returns 0 before registration.
/// — GraveShift: raw TSC / CNTPCT / CP0 Count. Not calibrated. For
/// calibrated nanoseconds use os_core::now_ns().
#[inline]
pub fn read_tsc() -> u64 {
    let f = READ_TSC_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() -> u64 = unsafe { core::mem::transmute(f) };
        func()
    } else {
        0
    }
}

/// Execute CPUID query. Returns (0,0,0,0) before registration.
/// — GraveShift: leaf/subleaf are x86 EAX/ECX inputs. Other arches adapt.
#[inline]
pub fn cpuid(leaf: u32, subleaf: u32) -> (u32, u32, u32, u32) {
    let f = CPUID_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn(u32, u32) -> (u32, u32, u32, u32) = unsafe { core::mem::transmute(f) };
        func(leaf, subleaf)
    } else {
        (0, 0, 0, 0)
    }
}

/// Full memory fence. No-op before registration.
/// — GraveShift: mfence / dmb ish / sync. Serializes everything.
#[inline]
pub fn memory_fence() {
    let f = MEMORY_FENCE_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

/// Read memory fence. No-op before registration.
/// — GraveShift: lfence / dmb ishld / sync. Serializes loads.
#[inline]
pub fn read_fence() {
    let f = READ_FENCE_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

// ============================================================================
// TLB operations
//
// — NeonRoot: subsystem crates (mm-paging, smp/tlb) need TLB flush/invalidate
// without cfg-gating on an arch type. Same function-pointer pattern.
// ============================================================================

static TLB_FLUSH_FN: AtomicUsize = AtomicUsize::new(0);
static TLB_FLUSH_ALL_FN: AtomicUsize = AtomicUsize::new(0);

/// Register TLB operation hooks. Called once from kernel init.
pub fn register_tlb_ops(
    flush: fn(u64),
    flush_all: fn(),
) {
    TLB_FLUSH_FN.store(flush as usize, Ordering::Release);
    TLB_FLUSH_ALL_FN.store(flush_all as usize, Ordering::Release);
}

/// Flush TLB entry for a specific virtual address. No-op before registration.
#[inline]
pub fn tlb_flush(addr: u64) {
    let f = TLB_FLUSH_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn(u64) = unsafe { core::mem::transmute(f) };
        func(addr);
    }
}

/// Flush entire TLB. No-op before registration.
#[inline]
pub fn tlb_flush_all() {
    let f = TLB_FLUSH_ALL_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() = unsafe { core::mem::transmute(f) };
        func();
    }
}

// ============================================================================
// Page table root operations
//
// — NeonRoot: mm-paging needs to read/write the page table root register
// (CR3 on x86, TTBR0 on ARM) without cfg-gating on arch. Same atomic
// function-pointer pattern as TLB hooks. The kernel is the only place that
// knows the concrete arch — subsystem crates stay blissfully ignorant.
// ============================================================================

static PAGE_TABLE_ROOT_READ_FN: AtomicUsize = AtomicUsize::new(0);
static PAGE_TABLE_ROOT_WRITE_FN: AtomicUsize = AtomicUsize::new(0);

/// Register page table root operation hooks. Called once from kernel init.
pub fn register_page_table_root_ops(
    read_root: fn() -> u64,
    write_root: unsafe fn(u64),
) {
    PAGE_TABLE_ROOT_READ_FN.store(read_root as usize, Ordering::Release);
    PAGE_TABLE_ROOT_WRITE_FN.store(write_root as usize, Ordering::Release);
}

/// Read the current page table root physical address (CR3 / TTBR0).
/// Returns 0 before registration — callers beware. — NeonRoot
#[inline]
pub fn read_page_table_root() -> u64 {
    let f = PAGE_TABLE_ROOT_READ_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() -> u64 = unsafe { core::mem::transmute(f) };
        func()
    } else {
        0
    }
}

/// Write a new page table root (switch page tables).
/// No-op before registration. — NeonRoot
///
/// # Safety
/// The physical address must point to a valid, properly aligned page table.
#[inline]
pub unsafe fn write_page_table_root(root: u64) {
    let f = PAGE_TABLE_ROOT_WRITE_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: unsafe fn(u64) = unsafe { core::mem::transmute(f) };
        unsafe { func(root) };
    }
}

// ============================================================================
// SMP operations
//
// — NeonRoot: SMP crate needs IPI send, AP boot, CPU ID, timing — all arch-
// specific. These hooks eliminate every last cfg(target_arch) from smp/.
// ============================================================================

static SMP_SEND_IPI_TO_FN: AtomicUsize = AtomicUsize::new(0);
static SMP_SEND_IPI_BROADCAST_FN: AtomicUsize = AtomicUsize::new(0);
static SMP_SEND_IPI_SELF_FN: AtomicUsize = AtomicUsize::new(0);
static SMP_BOOT_AP_FN: AtomicUsize = AtomicUsize::new(0);
static SMP_CPU_ID_FN: AtomicUsize = AtomicUsize::new(0);
static SMP_DELAY_US_FN: AtomicUsize = AtomicUsize::new(0);
static SMP_MONOTONIC_COUNTER_FN: AtomicUsize = AtomicUsize::new(0);
static SMP_MONOTONIC_FREQ_FN: AtomicUsize = AtomicUsize::new(0);

/// Register SMP operation hooks. Called once from kernel init.
pub fn register_smp_ops(
    send_ipi_to: fn(u32, u8),
    send_ipi_broadcast: fn(u8, bool),
    send_ipi_self: fn(u8),
    boot_ap: fn(u32, u8),
    cpu_id: fn() -> Option<u32>,
    delay_us: fn(u64),
    monotonic_counter: fn() -> u64,
    monotonic_freq: fn() -> u64,
) {
    SMP_SEND_IPI_TO_FN.store(send_ipi_to as usize, Ordering::Release);
    SMP_SEND_IPI_BROADCAST_FN.store(send_ipi_broadcast as usize, Ordering::Release);
    SMP_SEND_IPI_SELF_FN.store(send_ipi_self as usize, Ordering::Release);
    SMP_BOOT_AP_FN.store(boot_ap as usize, Ordering::Release);
    SMP_CPU_ID_FN.store(cpu_id as usize, Ordering::Release);
    SMP_DELAY_US_FN.store(delay_us as usize, Ordering::Release);
    SMP_MONOTONIC_COUNTER_FN.store(monotonic_counter as usize, Ordering::Release);
    SMP_MONOTONIC_FREQ_FN.store(monotonic_freq as usize, Ordering::Release);
}

/// Send IPI to a specific CPU by hardware ID + vector. No-op before registration.
#[inline]
pub fn smp_send_ipi_to(hw_id: u32, vector: u8) {
    let f = SMP_SEND_IPI_TO_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn(u32, u8) = unsafe { core::mem::transmute(f) };
        func(hw_id, vector);
    }
}

/// Broadcast IPI to all CPUs. No-op before registration.
#[inline]
pub fn smp_send_ipi_broadcast(vector: u8, include_self: bool) {
    let f = SMP_SEND_IPI_BROADCAST_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn(u8, bool) = unsafe { core::mem::transmute(f) };
        func(vector, include_self);
    }
}

/// Send IPI to self. No-op before registration.
#[inline]
pub fn smp_send_ipi_self(vector: u8) {
    let f = SMP_SEND_IPI_SELF_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn(u8) = unsafe { core::mem::transmute(f) };
        func(vector);
    }
}

/// Execute arch-specific AP boot sequence. No-op before registration.
#[inline]
pub fn smp_boot_ap(hw_id: u32, trampoline_page: u8) {
    let f = SMP_BOOT_AP_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn(u32, u8) = unsafe { core::mem::transmute(f) };
        func(hw_id, trampoline_page);
    }
}

/// Get current CPU ID from hardware. Returns None before registration.
#[inline]
pub fn smp_cpu_id() -> Option<u32> {
    let f = SMP_CPU_ID_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() -> Option<u32> = unsafe { core::mem::transmute(f) };
        func()
    } else {
        None
    }
}

/// Busy-wait for N microseconds. No-op before registration.
#[inline]
pub fn smp_delay_us(us: u64) {
    let f = SMP_DELAY_US_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn(u64) = unsafe { core::mem::transmute(f) };
        func(us);
    }
}

/// Read monotonic hardware counter (TSC/CNTPCT). Returns 0 before registration.
#[inline]
pub fn smp_monotonic_counter() -> u64 {
    let f = SMP_MONOTONIC_COUNTER_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() -> u64 = unsafe { core::mem::transmute(f) };
        func()
    } else {
        0
    }
}

/// Get monotonic counter frequency in Hz. Returns 0 before registration.
#[inline]
pub fn smp_monotonic_freq() -> u64 {
    let f = SMP_MONOTONIC_FREQ_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() -> u64 = unsafe { core::mem::transmute(f) };
        func()
    } else {
        0
    }
}

// ============================================================================
// KernelMutex — preemption-aware spinlock
//
// — GraveShift: This is the Linux model. spin_lock() disables preemption,
// spin_unlock() re-enables it. The scheduler timer ISR checks preempt_count
// before yanking a task — if count > 0, something holds a lock, and preempting
// would deadlock the next task that tries the same lock. Simple. Effective.
// Took us 67 builds to figure out we needed it.
// ============================================================================

/// Preemption-aware spinlock. Disables preemption on lock, re-enables on unlock.
///
/// — GraveShift: Use this instead of raw spin::Mutex for any lock that could be
/// held when the timer ISR fires. The heap allocator is the poster child — every
/// Vec::push, Box::new, and format!() goes through it. Without preemption
/// protection, the scheduler can yank us mid-alloc, hand the CPU to another task
/// that also needs the heap, and boom — permanent deadlock. KernelMutex makes
/// that impossible by telling the scheduler "not now" while the lock is held.
pub struct KernelMutex<T: ?Sized> {
    inner: spin::Mutex<T>,
}

// SAFETY: KernelMutex has the same Send/Sync bounds as spin::Mutex
unsafe impl<T: ?Sized + Send> Send for KernelMutex<T> {}
unsafe impl<T: ?Sized + Send> Sync for KernelMutex<T> {}

/// Guard returned by KernelMutex::lock(). Dropping re-enables preemption.
///
/// — GraveShift: The Drop impl is critical. We MUST drop the inner guard
/// (releasing the spinlock) BEFORE calling preempt_enable(). Otherwise there's
/// a window where preemption is enabled but the lock is still held — exactly
/// the bug we're trying to prevent.
pub struct KernelMutexGuard<'a, T: ?Sized> {
    // Order matters: Rust drops fields in declaration order.
    // inner guard drops first (releases spinlock), then _preempt_token
    // triggers preempt_enable in its Drop.
    guard: spin::MutexGuard<'a, T>,
    _preempt_token: PreemptToken,
}

/// Token that calls preempt_enable on drop. Ensures correct ordering.
struct PreemptToken;

impl Drop for PreemptToken {
    #[inline]
    fn drop(&mut self) {
        call_preempt_enable();
    }
}

impl<T> KernelMutex<T> {
    /// Create a new KernelMutex wrapping the given value.
    pub const fn new(val: T) -> Self {
        Self {
            inner: spin::Mutex::new(val),
        }
    }

    /// Lock the mutex, disabling preemption until the guard is dropped.
    /// — GraveShift: preempt_disable BEFORE spin. If we spin first and get
    /// preempted mid-spin, we waste the whole timeslice spinning. Worse: the
    /// lock holder might be on the same CPU and can't make progress because
    /// we stole its timeslice to spin. Classic priority inversion.
    #[inline]
    pub fn lock(&self) -> KernelMutexGuard<'_, T> {
        call_preempt_disable();
        KernelMutexGuard {
            guard: self.inner.lock(),
            _preempt_token: PreemptToken,
        }
    }

    /// Try to lock the mutex without blocking.
    /// Disables preemption on success, leaves it unchanged on failure.
    #[inline]
    pub fn try_lock(&self) -> Option<KernelMutexGuard<'_, T>> {
        call_preempt_disable();
        match self.inner.try_lock() {
            Some(guard) => Some(KernelMutexGuard {
                guard,
                _preempt_token: PreemptToken,
            }),
            None => {
                // — GraveShift: Lock contended. Undo the preempt_disable since
                // we're not holding anything. Caller can retry or bail.
                call_preempt_enable();
                None
            }
        }
    }
}

impl<T: ?Sized> Deref for KernelMutexGuard<'_, T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &*self.guard
    }
}

impl<T: ?Sized> DerefMut for KernelMutexGuard<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        &mut *self.guard
    }
}

// ============================================================================
// ELF machine constant
//
// — NeonRoot: module loader needs to validate ELF e_machine without cfg gates.
// Registered once at boot — returns the native ELF machine type (EM_X86_64=62,
// EM_AARCH64=183, etc.). Returns 0 before registration.
// ============================================================================

static ELF_MACHINE_FN: AtomicUsize = AtomicUsize::new(0);

/// Register the native ELF machine type provider.
pub fn register_elf_machine(f: fn() -> u16) {
    ELF_MACHINE_FN.store(f as usize, Ordering::Release);
}

/// Get the native ELF machine type. Returns 0 before registration. — NeonRoot
#[inline]
pub fn elf_machine() -> u16 {
    let f = ELF_MACHINE_FN.load(Ordering::Acquire);
    if f != 0 {
        let func: fn() -> u16 = unsafe { core::mem::transmute(f) };
        func()
    } else {
        0
    }
}
